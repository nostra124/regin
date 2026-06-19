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

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::approval::{self, ApprovalRequest};
use crate::db;

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
}

impl Greeting {
    /// Whether anything needs the operator's attention.
    pub fn has_actions(&self) -> bool {
        !self.pending_changes.is_empty() || !self.decision_problems.is_empty()
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

    Ok(Greeting {
        mode: mode.to_string(),
        open_incidents,
        open_problems,
        pending_changes,
        decision_problems,
    })
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
