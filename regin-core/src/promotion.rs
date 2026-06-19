//! Promotion + demotion loop (FEAT-051 / DISC-015).
//!
//! regin distils *stable* LLM verdicts into cheap **deterministic checks** held in
//! a separate machine-managed store (never written into the human-authored to-be
//! state). Promotion criteria are regin-owned and grounded in both
//! N-consistency + confidence and a statistical error-bound; the promotion-error
//! KPI governs them. A promoted check that is later contradicted/overridden is
//! **immediately demoted**. (The periodic wide-lens re-audit is FEAT-055.)

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::kpi;

/// A deterministic check distilled from a stable LLM verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedCheck {
    pub id: String,
    pub domain: String,
    /// What the check keys on (e.g. an assertion key / verdict signature).
    pub signature: String,
    pub description: String,
    /// `active` | `demoted`.
    pub status: String,
    pub created_at: String,
    pub demoted_at: Option<String>,
    pub demote_reason: Option<String>,
}

/// Evidence behind a candidate promotion.
#[derive(Debug, Clone, Copy)]
pub struct VerdictStats {
    /// Consecutive consistent verdicts for the signature.
    pub consistent_count: usize,
    /// Model confidence in the verdict, in `[0, 1]`.
    pub confidence: f64,
    /// Statistical error-bound estimate for the verdict, in `[0, 1]`.
    pub error_bound: f64,
}

/// Criteria a candidate must clear to be promoted.
#[derive(Debug, Clone, Copy)]
pub struct PromotionPolicy {
    pub min_consistent: usize,
    pub min_confidence: f64,
    pub max_error_bound: f64,
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self {
            min_consistent: 5,
            min_confidence: 0.9,
            max_error_bound: 0.05,
        }
    }
}

/// Whether the evidence clears the promotion bar: enough consistent verdicts,
/// high-enough confidence, and a tight-enough error bound.
pub fn should_promote(stats: VerdictStats, policy: PromotionPolicy) -> bool {
    stats.consistent_count >= policy.min_consistent
        && stats.confidence >= policy.min_confidence
        && stats.error_bound <= policy.max_error_bound
}

const COLS: &str = "id, domain, signature, description, status, created_at, demoted_at, demote_reason";

fn row(r: &rusqlite::Row) -> rusqlite::Result<DerivedCheck> {
    Ok(DerivedCheck {
        id: r.get(0)?,
        domain: r.get(1)?,
        signature: r.get(2)?,
        description: r.get(3)?,
        status: r.get(4)?,
        created_at: r.get(5)?,
        demoted_at: r.get(6)?,
        demote_reason: r.get(7)?,
    })
}

/// Promote a stable verdict into the derived-checks store (status = active) and
/// count the promotion KPI.
pub fn promote(
    conn: &Connection,
    domain: &str,
    signature: &str,
    description: &str,
) -> Result<DerivedCheck> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO derived_checks (id, domain, signature, description, status, created_at, demoted_at, demote_reason)
         VALUES (?1, ?2, ?3, ?4, 'active', ?5, NULL, NULL)",
        params![&id, domain, signature, description, &now],
    )?;
    kpi::kpi_record(conn, kpi::M_PROMOTION, 1.0, Some(signature))?;
    let sql = format!("SELECT {COLS} FROM derived_checks WHERE id = ?1");
    conn.query_row(&sql, params![&id], row).context("derived check vanished after insert")
}

/// Promote only if the evidence clears `policy`. Returns the new check, or `None`.
pub fn maybe_promote(
    conn: &Connection,
    domain: &str,
    signature: &str,
    description: &str,
    stats: VerdictStats,
    policy: PromotionPolicy,
) -> Result<Option<DerivedCheck>> {
    if should_promote(stats, policy) {
        Ok(Some(promote(conn, domain, signature, description)?))
    } else {
        Ok(None)
    }
}

/// Active (non-demoted) derived checks.
pub fn active_checks(conn: &Connection) -> Result<Vec<DerivedCheck>> {
    let sql = format!("SELECT {COLS} FROM derived_checks WHERE status = 'active' ORDER BY created_at");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Demote a derived check (status = demoted) and count the promotion-error KPI.
pub fn demote(conn: &Connection, id: &str, reason: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let n = conn.execute(
        "UPDATE derived_checks SET status = 'demoted', demoted_at = ?1, demote_reason = ?2
         WHERE id = ?3 AND status = 'active'",
        params![&now, reason, id],
    )?;
    if n > 0 {
        kpi::kpi_record(conn, kpi::M_PROMOTION_ERROR, 1.0, Some(reason))?;
    }
    Ok(())
}

/// Demotion hook: a real-world contradiction/override demotes every active check
/// on the given signature. Returns how many were demoted.
pub fn on_contradiction(conn: &Connection, signature: &str, reason: &str) -> Result<usize> {
    let ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM derived_checks WHERE signature = ?1 AND status = 'active'")?;
        stmt.query_map(params![signature], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    for id in &ids {
        demote(conn, id, reason)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    #[test]
    fn promotion_criteria() {
        let p = PromotionPolicy::default();
        assert!(should_promote(VerdictStats { consistent_count: 5, confidence: 0.9, error_bound: 0.05 }, p));
        // each criterion can independently block
        assert!(!should_promote(VerdictStats { consistent_count: 4, confidence: 0.99, error_bound: 0.0 }, p));
        assert!(!should_promote(VerdictStats { consistent_count: 9, confidence: 0.8, error_bound: 0.0 }, p));
        assert!(!should_promote(VerdictStats { consistent_count: 9, confidence: 0.99, error_bound: 0.2 }, p));
    }

    #[test]
    fn promote_inserts_active_check_and_kpi() {
        let c = conn();
        let epoch = "1970-01-01T00:00:00Z";
        let chk = promote(&c, "disk", "disk.root.use_percent<90", "root stays under 90%").unwrap();
        assert_eq!(chk.status, "active");
        assert_eq!(active_checks(&c).unwrap().len(), 1);
        assert_eq!(kpi::kpi_count(&c, kpi::M_PROMOTION, epoch).unwrap(), 1);
    }

    #[test]
    fn maybe_promote_respects_criteria() {
        let c = conn();
        let weak = VerdictStats { consistent_count: 1, confidence: 0.5, error_bound: 0.3 };
        assert!(maybe_promote(&c, "d", "s", "x", weak, PromotionPolicy::default()).unwrap().is_none());
        let strong = VerdictStats { consistent_count: 6, confidence: 0.95, error_bound: 0.01 };
        assert!(maybe_promote(&c, "d", "s", "x", strong, PromotionPolicy::default()).unwrap().is_some());
        assert_eq!(active_checks(&c).unwrap().len(), 1);
    }

    #[test]
    fn demote_marks_and_counts_error_once() {
        let c = conn();
        let epoch = "1970-01-01T00:00:00Z";
        let chk = promote(&c, "d", "sig", "x").unwrap();
        demote(&c, &chk.id, "contradicted by reality").unwrap();
        assert!(active_checks(&c).unwrap().is_empty());
        assert_eq!(kpi::kpi_count(&c, kpi::M_PROMOTION_ERROR, epoch).unwrap(), 1);
        // demoting again is a no-op (already demoted) — no double count
        demote(&c, &chk.id, "again").unwrap();
        assert_eq!(kpi::kpi_count(&c, kpi::M_PROMOTION_ERROR, epoch).unwrap(), 1);
    }

    #[test]
    fn contradiction_demotes_all_on_signature() {
        let c = conn();
        promote(&c, "d", "sig-A", "one").unwrap();
        promote(&c, "d", "sig-A", "dup view").unwrap();
        promote(&c, "d", "sig-B", "other").unwrap();
        let n = on_contradiction(&c, "sig-A", "overridden").unwrap();
        assert_eq!(n, 2);
        let active = active_checks(&c).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].signature, "sig-B");
    }
}
