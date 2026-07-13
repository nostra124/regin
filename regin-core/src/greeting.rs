//! Standalone parking + login greeting (FEAT-043 / DISC-010).
//!
//! When effectively standalone (FEAT-041), decision/approval items are *parked*
//! locally rather than pushed — pending_approval changes (FEAT-037) and open
//! problems simply wait in the store. `regin chat` opens with a **greeting**: a
//! one-line health summary plus the actionable items, so there is a pull-at-login
//! channel when no supervisor bus is reachable.
//!
//! On bus recovery the parked items are **re-validated** ([`revalidate_parked`]):
//! anything whose incident has self-resolved is closed and dropped, and the rest
//! are turned into approval requests (FEAT-042) for the supervisor.
//!
//! **FEAT-069 extension**: the greeting also surfaces parked intent
//! escalations (`escalation_routing::pending_escalations` — human/regin-
//! sourced red goals, FEAT-066/069) as further action items, and
//! [`intent_rag_summary`] gives `regin metrics` the same RAG counts.

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::approval::{self, ApprovalRequest};
use crate::db;
use crate::escalation_routing;
use crate::goal;
use crate::objective;

/// One actionable item shown at login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionItem {
    /// `change` (awaiting approval) or `problem` (needs a decision).
    pub kind: String,
    pub id: String,
    pub title: String,
}

/// The login greeting: health line + actionable items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Greeting {
    pub mode: String,
    pub open_incidents: i64,
    pub open_problems: i64,
    /// Changes staged awaiting approval.
    pub pending_changes: Vec<ActionItem>,
    /// Open problems needing a human decision.
    pub decision_problems: Vec<ActionItem>,
    /// Endangered goals escalated by the planning control loop and parked
    /// for review (FEAT-066/069) — human- and regin-sourced escalations;
    /// dvalin-sourced ones go straight over the bus and aren't parked here.
    pub intent_escalations: Vec<ActionItem>,
}

impl Greeting {
    /// Whether anything needs the operator's attention.
    pub fn has_actions(&self) -> bool {
        !self.pending_changes.is_empty() || !self.decision_problems.is_empty() || !self.intent_escalations.is_empty()
    }

    /// The one-line health summary.
    pub fn health_line(&self) -> String {
        format!(
            "mode={} | {} open incident(s), {} open problem(s)",
            self.mode, self.open_incidents, self.open_problems
        )
    }
}

fn is_open_incident(status: &str) -> bool {
    matches!(status, "open" | "investigating" | "blocked")
}

/// Build the greeting from the store for the given effective `mode`.
pub fn build(conn: &Connection, mode: &str) -> Result<Greeting> {
    let open_incidents = db::incident_list(conn, None)?
        .into_iter()
        .filter(|i| is_open_incident(&i.status))
        .count() as i64;

    let open_problems_list = db::problem_list(conn, Some("open"))?;
    let open_problems = open_problems_list.len() as i64;

    let pending_changes = approval::pending_for_approval(conn)?
        .into_iter()
        .map(|c| ActionItem { kind: "change".into(), id: c.id, title: c.title })
        .collect();

    let decision_problems = open_problems_list
        .into_iter()
        .map(|p| ActionItem { kind: "problem".into(), id: p.id, title: p.title })
        .collect();

    let intent_escalations = escalation_routing::pending_escalations(conn)?
        .into_iter()
        .map(|e| ActionItem { kind: "intent_escalation".into(), id: e.goal_id, title: e.reason })
        .collect();

    Ok(Greeting {
        mode: mode.to_string(),
        open_incidents,
        open_problems,
        pending_changes,
        decision_problems,
        intent_escalations,
    })
}

/// RAG counts across every stored goal and objective — what `regin metrics`
/// (and the greeting) surface for the intent plane (acceptance criterion 3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentRagSummary {
    pub goals_green: usize,
    pub goals_amber: usize,
    pub goals_red: usize,
    pub objectives_green: usize,
    pub objectives_amber: usize,
    pub objectives_red: usize,
}

fn bump(summary: &mut usize, amber: &mut usize, red: &mut usize, rag: &str) {
    match rag {
        "green" => *summary += 1,
        "amber" => *amber += 1,
        "red" => *red += 1,
        _ => {}
    }
}

/// Compute [`IntentRagSummary`] from the current `goals`/`objectives`
/// tables.
pub fn intent_rag_summary(conn: &Connection) -> Result<IntentRagSummary> {
    let mut s = IntentRagSummary::default();
    for g in goal::goal_list(conn, None)? {
        bump(&mut s.goals_green, &mut s.goals_amber, &mut s.goals_red, &g.rag);
    }
    for o in objective::objective_list(conn)? {
        bump(&mut s.objectives_green, &mut s.objectives_amber, &mut s.objectives_red, &o.rag);
    }
    Ok(s)
}

/// Re-validate parked approval items on bus recovery: a pending change whose
/// incident has self-resolved is closed and dropped (no point bothering the
/// supervisor); the rest become approval requests to flush (FEAT-042/043).
pub fn revalidate_parked(conn: &Connection, reply_to: &str) -> Result<Vec<ApprovalRequest>> {
    let mut requests = Vec::new();
    for change in approval::pending_for_approval(conn)? {
        if let Some(inc_id) = &change.incident_id
            && let Some(inc) = db::incident_get(conn, inc_id)?
            && matches!(inc.status.as_str(), "resolved" | "closed")
        {
            // The deviation cleared on its own — drop the stale parked change.
            db::change_close(conn, &change.id)?;
            continue;
        }
        requests.push(approval::build_request(&change, reply_to));
    }
    Ok(requests)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn stage_change(c: &Connection, title: &str) -> (String, String) {
        let inc = db::incident_open(c, title, "", "high", "monitor", Some("svc")).unwrap();
        let chg = db::change_record(c, title, "fix", Some(&inc.id), None, None, None).unwrap();
        db::change_request_approval(c, &chg.id).unwrap();
        (inc.id, chg.id)
    }

    #[test]
    fn greeting_surfaces_health_and_actions() {
        let c = conn();
        // an open incident, a pending change, and an open problem
        let _ = db::incident_open(&c, "disk", "", "high", "monitor", Some("disk")).unwrap();
        let (_inc, chg) = stage_change(&c, "restart svc");
        let prob = db::problem_open(&c, "recurring crash", "?").unwrap();

        let g = build(&c, "standalone").unwrap();
        assert!(g.has_actions());
        assert!(g.health_line().contains("standalone"));
        assert_eq!(g.open_incidents, 2, "raw incident + the staged change's incident");
        assert_eq!(g.open_problems, 1);
        assert_eq!(g.pending_changes.len(), 1);
        assert_eq!(g.pending_changes[0].id, chg);
        assert_eq!(g.decision_problems[0].id, prob.id);
    }

    #[test]
    fn quiet_system_has_no_actions() {
        let c = conn();
        let g = build(&c, "org").unwrap();
        assert!(!g.has_actions());
        assert_eq!(g.open_incidents, 0);
        assert_eq!(g.health_line(), "mode=org | 0 open incident(s), 0 open problem(s)");
    }

    #[test]
    fn greeting_surfaces_parked_intent_escalations() {
        // acceptance criterion 3
        let c = conn();
        let esc = crate::control_loop::PlanningEscalation {
            goal_id: "goal-1".into(),
            source: "human".into(),
            reason: "tasks still failing after mitigate/replan: t1".into(),
            remedies: crate::control_loop::standard_remedies(),
        };
        escalation_routing::park_escalation(&c, &esc).unwrap();

        let g = build(&c, "standalone").unwrap();
        assert!(g.has_actions());
        assert_eq!(g.intent_escalations.len(), 1);
        assert_eq!(g.intent_escalations[0].id, "goal-1");
        assert_eq!(g.intent_escalations[0].kind, "intent_escalation");
    }

    #[test]
    fn intent_rag_summary_counts_goals_and_objectives_by_rag() {
        let c = conn();
        let deadline = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let g1 = goal::goal_create(&c, "d", "t", &deadline, vec![], 1, "human").unwrap();
        goal::goal_activate(&c, &g1.id).unwrap();
        let _g2 = goal::goal_create(&c, "d2", "t2", &deadline, vec![], 2, "human").unwrap(); // stays green (proposed)

        objective::objective_create(
            &c, "t", "d", "m", "sum", 30, "le", &crate::desired::AssertValue::Num(1.0), 1, "human",
        ).unwrap();

        let summary = intent_rag_summary(&c).unwrap();
        assert_eq!(summary.goals_green, 2, "both goals start green");
        assert_eq!(summary.goals_red, 0);
        assert_eq!(summary.objectives_green, 1, "untested objectives start green");
    }

    #[test]
    fn revalidate_drops_self_resolved_and_flushes_rest() {
        let c = conn();
        let (inc_keep, chg_keep) = stage_change(&c, "still broken");
        let (inc_gone, chg_gone) = stage_change(&c, "fixed itself");
        // the second incident self-resolved while parked
        db::incident_resolve(&c, &inc_gone, "recovered on its own").unwrap();
        let _ = inc_keep;

        let reqs = revalidate_parked(&c, "supervisor@hq").unwrap();
        assert_eq!(reqs.len(), 1, "only the still-relevant change is flushed");
        assert_eq!(reqs[0].change_id, chg_keep);
        // the stale parked change was closed
        assert_eq!(db::change_get(&c, &chg_gone).unwrap().unwrap().status, "closed");
        // and it no longer shows as pending
        assert_eq!(approval::pending_for_approval(&c).unwrap().len(), 1);
    }
}
