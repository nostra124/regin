//! Decision/approval escalation (FEAT-042 / DISC-010).
//!
//! A *second* escalation flavour atop [`crate::escalation`]: where that asks the
//! dev plane to mint a ticket, this asks a human/supervisor for a **decision** on
//! a change staged `pending_approval` (FEAT-037). In org mode (FEAT-041) the
//! request goes to the supervisor over the bus; the verdict resumes the change
//! lifecycle. Like the escalation module, the payload is pure and unit-tested and
//! the bus send is wired by the caller.

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::types::Change;
use crate::{db, kpi};

/// The structured approval-request payload (body of a `structured` bus message).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Marks this structured message as an approval request for the router.
    #[serde(default = "approval_tag")]
    pub kind: String,
    pub change_id: String,
    pub title: String,
    pub description: String,
    /// The captured rollback plan, so the approver can weigh reversibility.
    pub backout: Option<String>,
    /// The incident this change remediates, if any.
    pub incident_id: Option<String>,
    /// Where the verdict should be reported back to.
    pub reply_to: String,
    /// Correlation ref echoed on the verdict.
    pub ref_id: String,
}

fn approval_tag() -> String {
    "approval_request".to_string()
}

/// The correlation ref regin assigns to an approval request for `change_id`.
pub fn correlation_ref(change_id: &str) -> String {
    format!("APR-{change_id}")
}

/// Build an approval request from a staged change.
pub fn build_request(change: &Change, reply_to: &str) -> ApprovalRequest {
    ApprovalRequest {
        kind: approval_tag(),
        change_id: change.id.clone(),
        title: change.title.clone(),
        description: change.description.clone(),
        backout: change.before.clone(),
        incident_id: change.incident_id.clone(),
        reply_to: reply_to.to_string(),
        ref_id: correlation_ref(&change.id),
    }
}

/// Serialize an approval request as a structured bus-message body.
pub fn body(req: &ApprovalRequest) -> Result<String> {
    Ok(serde_json::to_string(req)?)
}

/// A supervisor's verdict on an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalVerdict {
    pub change_id: String,
    pub approved: bool,
    pub approved_by: String,
}

/// Changes currently staged awaiting a decision (FEAT-037).
pub fn pending_for_approval(conn: &Connection) -> Result<Vec<Change>> {
    Ok(db::change_list(conn)?
        .into_iter()
        .filter(|c| c.status == "pending_approval")
        .collect())
}

/// Apply a verdict: approve → stamp the approver, apply the change, count the
/// `remediation.approved` KPI; reject → close the change unapplied (DISC-010).
pub fn apply_verdict(conn: &Connection, verdict: &ApprovalVerdict) -> Result<()> {
    if verdict.approved {
        db::change_approve(conn, &verdict.change_id, &verdict.approved_by)?;
        db::change_apply(conn, &verdict.change_id)?;
        kpi::kpi_record(conn, kpi::M_REMEDIATION_APPROVED, 1.0, Some(&verdict.change_id))?;
        db::episode_record(
            conn,
            "change",
            Some(&verdict.change_id),
            &format!("change approved by {} and applied", verdict.approved_by),
            None,
        )?;
    } else {
        db::change_close(conn, &verdict.change_id)?;
        db::episode_record(
            conn,
            "change",
            Some(&verdict.change_id),
            &format!("change rejected by {}", verdict.approved_by),
            None,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn staged(c: &Connection) -> Change {
        let inc = db::incident_open(c, "svc down", "", "high", "monitor", Some("svc")).unwrap();
        let chg = db::change_record(c, "restart svc", "bring it back", Some(&inc.id), None, Some("snapshot s1"), None).unwrap();
        db::change_request_approval(c, &chg.id).unwrap();
        db::change_get(c, &chg.id).unwrap().unwrap()
    }

    #[test]
    fn builds_tagged_request_with_backout_and_ref() {
        let c = conn();
        let chg = staged(&c);
        let req = build_request(&chg, "supervisor@hq");
        assert_eq!(req.kind, "approval_request");
        assert_eq!(req.change_id, chg.id);
        assert_eq!(req.backout.as_deref(), Some("snapshot s1"));
        assert_eq!(req.reply_to, "supervisor@hq");
        assert_eq!(req.ref_id, format!("APR-{}", chg.id));
        // round-trips through the bus body
        let restored: ApprovalRequest = serde_json::from_str(&body(&req).unwrap()).unwrap();
        assert_eq!(restored, req);
    }

    #[test]
    fn pending_list_filters_to_staged_changes() {
        let c = conn();
        let chg = staged(&c);
        // a separate, already-applied change should not appear
        let other = db::change_record(&c, "x", "", None, None, None, None).unwrap();
        db::change_apply(&c, &other.id).unwrap();
        let pending = pending_for_approval(&c).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, chg.id);
    }

    #[test]
    fn approve_applies_and_counts_kpi() {
        let c = conn();
        let chg = staged(&c);
        apply_verdict(&c, &ApprovalVerdict { change_id: chg.id.clone(), approved: true, approved_by: "rene".into() }).unwrap();
        let after = db::change_get(&c, &chg.id).unwrap().unwrap();
        assert_eq!(after.status, "applied");
        assert_eq!(after.approved_by.as_deref(), Some("rene"));
        assert!(after.approved_at.is_some());
        assert_eq!(kpi::kpi_count(&c, kpi::M_REMEDIATION_APPROVED, "1970-01-01T00:00:00Z").unwrap(), 1);
        assert!(pending_for_approval(&c).unwrap().is_empty());
    }

    #[test]
    fn reject_closes_without_applying() {
        let c = conn();
        let chg = staged(&c);
        apply_verdict(&c, &ApprovalVerdict { change_id: chg.id.clone(), approved: false, approved_by: "rene".into() }).unwrap();
        let after = db::change_get(&c, &chg.id).unwrap().unwrap();
        assert_eq!(after.status, "closed");
        assert!(after.applied_at.is_none());
        assert_eq!(kpi::kpi_count(&c, kpi::M_REMEDIATION_APPROVED, "1970-01-01T00:00:00Z").unwrap(), 0);
    }
}
