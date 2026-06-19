//! FEAT-017 (DISC-006): regin's individual planning cycle. Each regin plans its
//! own work from its decentralized signals — per-repo schedules (the *When*),
//! per-repo required capabilities/skills (the *Which*), and its ITIL backlog — on
//! a cadence, then feeds the result up both matrix axes: priority asks sideways
//! to the project/process owner, capability gaps up the functional line to the
//! CAO (DISC-032).
//!
//! Pure aggregation + upward-signal message builders — unit-tested. The db reads
//! and the bus sends are wired by the caller.

use serde::{Deserialize, Serialize};

/// The cadences a regin plans on (DISC-006), and the planning scope each covers.
pub fn cadence_scope(cadence: &str) -> Option<&'static str> {
    match cadence {
        "weekly" => Some("operational backlog"),
        "monthly" => Some("capability + self-improvement review"),
        "yearly" => Some("strategic roll-up"),
        _ => None,
    }
}

/// An individual plan for one cadence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub cadence: String,
    pub scope: String,
    /// The *When*: scheduled task names due this cycle.
    pub scheduled: Vec<String>,
    /// The *Which* gaps: required capabilities not yet available.
    pub capability_gaps: Vec<String>,
    /// ITIL backlog carried.
    pub open_incidents: usize,
    pub open_problems: usize,
}

/// Build the plan: aggregate schedules + ITIL backlog, and compute capability
/// gaps as the required capabilities not present in the available set.
pub fn build_plan(
    cadence: &str,
    schedules: &[String],
    required_caps: &[String],
    available_caps: &[String],
    open_incidents: usize,
    open_problems: usize,
) -> Plan {
    let mut capability_gaps: Vec<String> = required_caps
        .iter()
        .filter(|c| !available_caps.contains(c))
        .cloned()
        .collect();
    capability_gaps.sort();
    capability_gaps.dedup();
    Plan {
        cadence: cadence.to_string(),
        scope: cadence_scope(cadence).unwrap_or("plan").to_string(),
        scheduled: schedules.to_vec(),
        capability_gaps,
        open_incidents,
        open_problems,
    }
}

/// The upward priority ask to the project/process owner (horizontal axis): the
/// scheduled work + backlog pressure this regin is carrying.
pub fn priority_ask_body(plan: &Plan) -> String {
    serde_json::json!({
        "kind": "priority_ask",
        "cadence": plan.cadence,
        "scheduled": plan.scheduled,
        "open_incidents": plan.open_incidents,
        "open_problems": plan.open_problems,
    })
    .to_string()
}

/// The upward capability-gap signal to the CAO (functional axis): the skills this
/// regin needs but lacks. Returns `None` when there is no gap (nothing to send).
pub fn capability_gap_body(plan: &Plan) -> Option<String> {
    if plan.capability_gaps.is_empty() {
        return None;
    }
    Some(
        serde_json::json!({
            "kind": "capability_gap",
            "cadence": plan.cadence,
            "gaps": plan.capability_gaps,
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_scopes() {
        assert!(cadence_scope("weekly").is_some());
        assert!(cadence_scope("monthly").is_some());
        assert!(cadence_scope("yearly").is_some());
        assert!(cadence_scope("hourly").is_none());
    }

    #[test]
    fn build_plan_computes_capability_gaps() {
        let plan = build_plan(
            "weekly",
            &["backup-nightly".to_string(), "monitor-disk".to_string()],
            &["regin-backup-skills".to_string(), "regin-cfo-skills".to_string(), "regin-backup-skills".to_string()],
            &["regin-backup-skills".to_string()],
            2,
            1,
        );
        assert_eq!(plan.scope, "operational backlog");
        assert_eq!(plan.scheduled.len(), 2);
        assert_eq!(plan.capability_gaps, vec!["regin-cfo-skills".to_string()], "required minus available, deduped");
        assert_eq!(plan.open_incidents, 2);
        assert_eq!(plan.open_problems, 1);
    }

    #[test]
    fn priority_ask_carries_schedule_and_backlog() {
        let plan = build_plan("weekly", &["t1".into()], &[], &[], 3, 0);
        let v: serde_json::Value = serde_json::from_str(&priority_ask_body(&plan)).unwrap();
        assert_eq!(v["kind"], "priority_ask");
        assert_eq!(v["open_incidents"], 3);
        assert_eq!(v["scheduled"][0], "t1");
    }

    #[test]
    fn capability_gap_only_when_there_is_a_gap() {
        let with_gap = build_plan("monthly", &[], &["x".into()], &[], 0, 0);
        assert!(capability_gap_body(&with_gap).is_some());
        let no_gap = build_plan("monthly", &[], &["x".into()], &["x".into()], 0, 0);
        assert!(capability_gap_body(&no_gap).is_none(), "nothing to send when fully capable");
    }
}
