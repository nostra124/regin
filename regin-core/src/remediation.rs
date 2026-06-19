//! Three-lane remediation engine (FEAT-037 / DISC-009).
//!
//! A judged deviation → incident produces a *candidate fix*, which this engine
//! routes into one of three lanes and records as ITIL artifacts + KPI events,
//! closing the loop that previously only reported:
//!
//! - **auto-apply** — a `Safe`, reversible fix the capability ceiling permits is
//!   applied and recorded as an applied change (the safe-lane gate, FEAT-039,
//!   supplies the reversibility/blast-radius judgement upstream);
//! - **pending_approval** — an uncertain/destructive fix is staged as a change
//!   awaiting a human/supervisor decision (routed by FEAT-042/043);
//! - **escalate** — a fix out of regin's control opens a *problem* and escalates.
//!
//! Like [`crate::escalation`], this is a pure-ish engine: the LLM risk judgement
//! for *novel* fixes ([`RiskJudge`]) and the act of executing the change are wired
//! by the caller (the evaluation loop). A declarative **safe-action fast-path**
//! ([`from_preblessed`]) classifies pre-blessed reversible ops without the LLM.

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{db, kpi};

/// The judged risk of a candidate fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskClass {
    /// Safe and reversible — eligible for the auto-apply lane.
    Safe,
    /// Uncertain or destructive — needs a human/supervisor decision.
    Uncertain,
    /// Outside regin's control — must be escalated.
    OutOfControl,
}

/// Which remediation lane a candidate fix is routed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lane {
    AutoApply,
    PendingApproval,
    Escalate,
}

impl std::fmt::Display for Lane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lane::AutoApply => write!(f, "auto-apply"),
            Lane::PendingApproval => write!(f, "pending_approval"),
            Lane::Escalate => write!(f, "escalate"),
        }
    }
}

/// A proposed remediation for a deviation/incident.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateFix {
    pub title: String,
    pub description: String,
    /// Whether a concrete backout/undo exists (the FEAT-039 safe-lane gate input).
    pub reversible: bool,
    pub risk: RiskClass,
}

/// Pre-blessed reversible operation tags that auto-qualify for the safe lane
/// without an LLM judgement (the declarative fast-path).
pub const PREBLESSED: &[&str] = &[
    "clear_temp",
    "rotate_logs",
    "compress_logs",
    "truncate_logs",
    "clean_pkg_cache",
    "restart_unit",
    "force_timesync",
    "run_backup",
];

/// Whether a remediation tag is on the pre-blessed safe-action allowlist.
pub fn is_preblessed(tag: &str) -> bool {
    PREBLESSED.contains(&tag)
}

/// Build a `Safe`, reversible candidate from a pre-blessed tag (the fast-path).
/// Returns `None` for an unknown tag (which must go through [`RiskJudge`]).
pub fn from_preblessed(tag: &str, title: &str, description: &str) -> Option<CandidateFix> {
    is_preblessed(tag).then(|| CandidateFix {
        title: title.to_string(),
        description: description.to_string(),
        reversible: true,
        risk: RiskClass::Safe,
    })
}

/// A judge for *novel* (non-pre-blessed) fixes. The LLM implementation is wired by
/// the loop; the conservative default treats the unknown as uncertain.
pub trait RiskJudge {
    fn judge(&self, fix: &CandidateFix) -> RiskClass;
}

/// Conservative default: never escalates a Safe verdict, but downgrades nothing —
/// it simply trusts a Safe-tagged fix and treats anything else as uncertain.
pub struct ConservativeJudge;

impl RiskJudge for ConservativeJudge {
    fn judge(&self, fix: &CandidateFix) -> RiskClass {
        match fix.risk {
            RiskClass::Safe => RiskClass::Safe,
            _ => RiskClass::Uncertain,
        }
    }
}

/// Route a candidate fix to a lane (FEAT-037). Out-of-control escalates; a Safe,
/// reversible fix auto-applies when `auto_permitted`; everything else needs
/// approval. `auto_permitted` is the AND of the auto-apply gates the caller
/// composes: the capability ceiling (FEAT-038) and the adaptive posture
/// (FEAT-040). A conservative posture makes this false, so safe fixes still route
/// to approval until trust is earned.
pub fn route(fix: &CandidateFix, auto_permitted: bool) -> Lane {
    match fix.risk {
        RiskClass::OutOfControl => Lane::Escalate,
        RiskClass::Safe if fix.reversible && auto_permitted => Lane::AutoApply,
        _ => Lane::PendingApproval,
    }
}

/// The result of routing+recording a remediation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemediationOutcome {
    pub lane: Lane,
    /// The change created (auto-apply / pending_approval lanes).
    pub change_id: Option<String>,
    /// The problem opened (escalate lane).
    pub problem_id: Option<String>,
}

/// Route a candidate fix for `incident_id`, recording the ITIL artifacts and KPI:
/// - **auto-apply**: an applied change (+ `remediation.auto`), with the captured
///   `backout` (FEAT-039) persisted on the change's `before` field for rollback;
/// - **pending_approval**: a change staged for approval (counted on approval);
/// - **escalate**: a problem linked to the incident (+ `remediation.escalated`).
///
/// `auto_permitted` is the AND of the auto-apply gates (capability ceiling
/// FEAT-038 and adaptive posture FEAT-040) the caller composes. `backout` is the
/// safe-lane gate's captured rollback plan, if any.
pub fn record_and_route(
    conn: &Connection,
    incident_id: &str,
    fix: &CandidateFix,
    auto_permitted: bool,
    backout: Option<&str>,
) -> Result<RemediationOutcome> {
    let lane = route(fix, auto_permitted);
    let outcome = match lane {
        Lane::AutoApply => {
            let change = db::change_record(
                conn,
                &fix.title,
                &fix.description,
                Some(incident_id),
                None,
                backout,
                None,
            )?;
            db::change_apply(conn, &change.id)?;
            kpi::kpi_record(conn, kpi::M_REMEDIATION_AUTO, 1.0, Some(&fix.title))?;
            db::episode_record(
                conn,
                "change",
                Some(&change.id),
                &format!("auto-applied remediation: {}", fix.title),
                Some(&fix.description),
            )?;
            RemediationOutcome { lane, change_id: Some(change.id), problem_id: None }
        }
        Lane::PendingApproval => {
            let change = db::change_record(
                conn,
                &fix.title,
                &fix.description,
                Some(incident_id),
                None,
                None,
                None,
            )?;
            db::change_request_approval(conn, &change.id)?;
            db::episode_record(
                conn,
                "change",
                Some(&change.id),
                &format!("remediation needs approval: {}", fix.title),
                Some(&fix.description),
            )?;
            RemediationOutcome { lane, change_id: Some(change.id), problem_id: None }
        }
        Lane::Escalate => {
            let problem = db::problem_open(
                conn,
                &format!("out-of-control deviation: {}", fix.title),
                &fix.description,
            )?;
            db::link_incident_to_problem(conn, &problem.id, incident_id)?;
            kpi::kpi_record(conn, kpi::M_REMEDIATION_ESCALATED, 1.0, Some(&fix.title))?;
            RemediationOutcome { lane, change_id: None, problem_id: Some(problem.id) }
        }
    };
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn fix(risk: RiskClass, reversible: bool) -> CandidateFix {
        CandidateFix { title: "t".into(), description: "d".into(), reversible, risk }
    }

    #[test]
    fn routing_covers_all_lanes() {
        // safe + reversible + ceiling -> auto
        assert_eq!(route(&fix(RiskClass::Safe, true), true), Lane::AutoApply);
        // safe but not reversible -> approval
        assert_eq!(route(&fix(RiskClass::Safe, false), true), Lane::PendingApproval);
        // safe + reversible but ceiling denies -> approval
        assert_eq!(route(&fix(RiskClass::Safe, true), false), Lane::PendingApproval);
        // uncertain -> approval
        assert_eq!(route(&fix(RiskClass::Uncertain, true), true), Lane::PendingApproval);
        // out of control -> escalate (regardless of ceiling/reversibility)
        assert_eq!(route(&fix(RiskClass::OutOfControl, true), true), Lane::Escalate);
    }

    #[test]
    fn preblessed_fast_path() {
        assert!(is_preblessed("rotate_logs"));
        assert!(!is_preblessed("reboot_host"));
        let c = from_preblessed("clear_temp", "clear /tmp", "remove temp files").unwrap();
        assert_eq!(c.risk, RiskClass::Safe);
        assert!(c.reversible);
        assert!(from_preblessed("rm_rf_root", "x", "y").is_none());
    }

    #[test]
    fn conservative_judge_only_trusts_safe() {
        let j = ConservativeJudge;
        assert_eq!(j.judge(&fix(RiskClass::Safe, true)), RiskClass::Safe);
        assert_eq!(j.judge(&fix(RiskClass::Uncertain, true)), RiskClass::Uncertain);
        assert_eq!(j.judge(&fix(RiskClass::OutOfControl, true)), RiskClass::Uncertain);
    }

    #[test]
    fn auto_apply_records_applied_change_and_kpi() {
        let c = conn();
        let inc = db::incident_open(&c, "disk full", "", "high", "monitor", Some("disk")).unwrap();
        let out = record_and_route(&c, &inc.id, &fix(RiskClass::Safe, true), true, Some("restore /tmp from snapshot s1")).unwrap();
        assert_eq!(out.lane, Lane::AutoApply);
        let chg = db::change_get(&c, out.change_id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(chg.status, "applied");
        assert_eq!(chg.incident_id.as_deref(), Some(inc.id.as_str()));
        assert_eq!(chg.before.as_deref(), Some("restore /tmp from snapshot s1"), "backout persisted for rollback");
        assert_eq!(kpi::kpi_count(&c, kpi::M_REMEDIATION_AUTO, "1970-01-01T00:00:00Z").unwrap(), 1);
    }

    #[test]
    fn uncertain_fix_is_staged_for_approval() {
        let c = conn();
        let inc = db::incident_open(&c, "config drift", "", "medium", "monitor", Some("svc")).unwrap();
        let out = record_and_route(&c, &inc.id, &fix(RiskClass::Uncertain, true), true, None).unwrap();
        assert_eq!(out.lane, Lane::PendingApproval);
        let chg = db::change_get(&c, out.change_id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(chg.status, "pending_approval");
        // not counted as auto
        assert_eq!(kpi::kpi_count(&c, kpi::M_REMEDIATION_AUTO, "1970-01-01T00:00:00Z").unwrap(), 0);
    }

    #[test]
    fn out_of_control_opens_problem_and_escalates() {
        let c = conn();
        let inc = db::incident_open(&c, "hardware fault", "", "critical", "monitor", Some("disk")).unwrap();
        let out = record_and_route(&c, &inc.id, &fix(RiskClass::OutOfControl, false), true, None).unwrap();
        assert_eq!(out.lane, Lane::Escalate);
        let pid = out.problem_id.unwrap();
        assert!(db::problem_get(&c, &pid).unwrap().is_some());
        assert_eq!(db::problem_incident_ids(&c, &pid).unwrap(), vec![inc.id.clone()]);
        assert_eq!(db::incident_problem_id(&c, &inc.id).unwrap().as_deref(), Some(pid.as_str()));
        assert_eq!(kpi::kpi_count(&c, kpi::M_REMEDIATION_ESCALATED, "1970-01-01T00:00:00Z").unwrap(), 1);
        assert!(out.change_id.is_none());
    }
}
