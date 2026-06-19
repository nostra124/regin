//! Safe-lane gate (FEAT-039 / DISC-009).
//!
//! A fix may only auto-apply (FEAT-037) when it clears this gate: a concrete
//! **backout/undo** must be captured (snapshot, backup, or a provably reversible
//! op), an available **dry-run** must have succeeded, and the **blast radius**
//! must be within bound. A change with no rollback plan can never auto-apply — it
//! falls to `pending_approval`. The captured backout is persisted on the change so
//! it can be executed to roll back (ITIL backout-plan discipline).

use serde::{Deserialize, Serialize};

/// How an auto-applied change would be rolled back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoutKind {
    /// A filesystem/VM snapshot to restore.
    Snapshot,
    /// A data backup to restore.
    Backup,
    /// The operation is inherently reversible by an inverse op.
    InverseOp,
    /// No rollback path — disqualifies the safe lane.
    None,
}

impl BackoutKind {
    pub fn is_concrete(&self) -> bool {
        !matches!(self, BackoutKind::None)
    }
}

/// The inputs to the safe-lane gate for a candidate fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafeLaneCheck {
    pub backout: BackoutKind,
    /// Human/machine-readable rollback plan, persisted on the change.
    pub backout_detail: Option<String>,
    /// Result of a dry-run, when the op supports one (`None` = not applicable).
    pub dry_run_ok: Option<bool>,
    /// Count of resources the op would affect (its scope).
    pub blast_radius: u32,
}

/// The gate's verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafeLaneVerdict {
    pub auto_ok: bool,
    pub reasons: Vec<String>,
}

/// Evaluate the safe-lane gate. `max_blast` bounds the affected-resource count.
pub fn evaluate(check: &SafeLaneCheck, max_blast: u32) -> SafeLaneVerdict {
    let mut reasons = Vec::new();
    if !check.backout.is_concrete() {
        reasons.push("no backout/rollback plan captured".to_string());
    }
    if check.dry_run_ok == Some(false) {
        reasons.push("dry-run failed".to_string());
    }
    if check.blast_radius > max_blast {
        reasons.push(format!(
            "blast radius {} exceeds bound {max_blast}",
            check.blast_radius
        ));
    }
    SafeLaneVerdict { auto_ok: reasons.is_empty(), reasons }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(backout: BackoutKind, dry: Option<bool>, blast: u32) -> SafeLaneCheck {
        SafeLaneCheck {
            backout,
            backout_detail: Some("restore snapshot s1".into()),
            dry_run_ok: dry,
            blast_radius: blast,
        }
    }

    #[test]
    fn clean_check_passes() {
        let v = evaluate(&check(BackoutKind::Snapshot, Some(true), 1), 10);
        assert!(v.auto_ok);
        assert!(v.reasons.is_empty());
        // dry-run not applicable is fine
        assert!(evaluate(&check(BackoutKind::InverseOp, None, 0), 10).auto_ok);
        assert!(evaluate(&check(BackoutKind::Backup, Some(true), 10), 10).auto_ok);
    }

    #[test]
    fn no_backout_disqualifies() {
        let v = evaluate(&check(BackoutKind::None, Some(true), 0), 10);
        assert!(!v.auto_ok);
        assert!(v.reasons.iter().any(|r| r.contains("backout")));
    }

    #[test]
    fn failed_dry_run_disqualifies() {
        let v = evaluate(&check(BackoutKind::Snapshot, Some(false), 0), 10);
        assert!(!v.auto_ok);
        assert!(v.reasons.iter().any(|r| r.contains("dry-run")));
    }

    #[test]
    fn over_blast_radius_disqualifies() {
        let v = evaluate(&check(BackoutKind::Snapshot, Some(true), 11), 10);
        assert!(!v.auto_ok);
        assert!(v.reasons.iter().any(|r| r.contains("blast radius")));
    }

    #[test]
    fn multiple_failures_all_reported() {
        let v = evaluate(&check(BackoutKind::None, Some(false), 99), 10);
        assert!(!v.auto_ok);
        assert_eq!(v.reasons.len(), 3);
    }
}
