//! KPI store + CSI metrics (FEAT-050 / DISC-015).
//!
//! KPIs span four groups: **reliability/quality** (derived from the ITIL tables),
//! **automation/autonomy**, **cost/efficiency**, and **learning/health** (the last
//! three accumulated as timestamped events emitted by later features — they read
//! zero until those features land, exactly as DISC-015 anticipates).
//!
//! The north-star is a *constrained objective*: minimize cost subject to
//! reliability ≥ a floor. [`objective`] reports whether that constraint holds.

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

// --- Metric names emitted by later features (FEAT-037/049/051/052). ---

/// LLM spend, in USD.
pub const M_COST_LLM: &str = "cost.llm_usd";
/// Estimated cost avoided by automation, in USD.
pub const M_COST_AVOIDED: &str = "cost.avoided_usd";
/// A notice dropped by a notice filter before reaching the LLM (FEAT-052).
pub const M_NOTICE_FILTER_SAVED: &str = "notice_filter.saved";
/// A remediation auto-applied in the safe lane (FEAT-037).
pub const M_REMEDIATION_AUTO: &str = "remediation.auto";
/// A remediation that required approval (FEAT-037/042).
pub const M_REMEDIATION_APPROVED: &str = "remediation.approved";
/// A deviation escalated as out-of-control (FEAT-037).
pub const M_REMEDIATION_ESCALATED: &str = "remediation.escalated";
/// An LLM verdict promoted to a deterministic check (FEAT-051).
pub const M_PROMOTION: &str = "promotion.count";
/// A promoted check later found wrong and demoted (FEAT-051).
pub const M_PROMOTION_ERROR: &str = "promotion.error";

/// Append a KPI event.
pub fn kpi_record(conn: &Connection, metric: &str, value: f64, meta: Option<&str>) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO kpi_events (id, recorded_at, metric, value, meta) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, &now, metric, value, meta],
    )?;
    Ok(())
}

/// Sum of a metric's event values at/after `since` (RFC3339).
pub fn kpi_sum(conn: &Connection, metric: &str, since: &str) -> Result<f64> {
    let v: f64 = conn.query_row(
        "SELECT COALESCE(SUM(value), 0.0) FROM kpi_events WHERE metric = ?1 AND recorded_at >= ?2",
        params![metric, since],
        |r| r.get(0),
    )?;
    Ok(v)
}

/// Count of a metric's events at/after `since` (RFC3339).
pub fn kpi_count(conn: &Connection, metric: &str, since: &str) -> Result<i64> {
    let v: i64 = conn.query_row(
        "SELECT COUNT(*) FROM kpi_events WHERE metric = ?1 AND recorded_at >= ?2",
        params![metric, since],
        |r| r.get(0),
    )?;
    Ok(v)
}

/// A computed KPI snapshot over a window starting at `since`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiSummary {
    pub since: String,
    // reliability / quality (from the ITIL tables)
    pub incidents_opened: i64,
    pub incidents_resolved: i64,
    pub open_incidents: i64,
    pub time_in_deviation_secs: i64,
    pub mttr_secs: Option<i64>,
    pub recurring_problems: i64,
    // automation / autonomy (from events)
    pub remediations_auto: i64,
    pub remediations_approved: i64,
    pub remediations_escalated: i64,
    pub automation_ratio: f64,
    pub autonomy_ratio: f64,
    // cost / efficiency (from events)
    pub cost_llm_usd: f64,
    pub cost_avoided_usd: f64,
    pub notice_filter_saved: i64,
    // learning / health (from events)
    pub promotions: i64,
    pub promotion_errors: i64,
    pub promotion_error_rate: f64,
}

fn parse_ts(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

/// Compute a KPI snapshot over `[since, now]`.
pub fn summary(conn: &Connection, since: &str) -> Result<KpiSummary> {
    let now = chrono::Utc::now();
    let since_dt = parse_ts(since).unwrap_or(now);

    // Reliability/quality from the incidents table.
    let incidents = crate::db::incident_list(conn, None)?;
    let mut incidents_opened = 0i64;
    let mut incidents_resolved = 0i64;
    let mut open_incidents = 0i64;
    let mut deviation_secs = 0i64;
    let mut resolve_durations: Vec<i64> = Vec::new();
    for inc in &incidents {
        let opened = match parse_ts(&inc.opened_at) {
            Some(t) => t,
            None => continue,
        };
        let in_window = opened >= since_dt;
        if in_window {
            incidents_opened += 1;
        }
        let is_open = matches!(inc.status.as_str(), "open" | "investigating" | "blocked");
        if is_open {
            open_incidents += 1;
        }
        // Time in deviation: opened -> resolved (or now), clamped to the window.
        let end = inc
            .resolved_at
            .as_deref()
            .and_then(parse_ts)
            .unwrap_or(now);
        let span_start = opened.max(since_dt);
        if end > span_start {
            deviation_secs += (end - span_start).num_seconds();
        }
        if let (true, Some(resolved)) = (in_window, inc.resolved_at.as_deref().and_then(parse_ts)) {
            incidents_resolved += 1;
            resolve_durations.push((resolved - opened).num_seconds().max(0));
        }
    }
    let mttr_secs = if resolve_durations.is_empty() {
        None
    } else {
        Some(resolve_durations.iter().sum::<i64>() / resolve_durations.len() as i64)
    };

    let recurring_problems = crate::db::problem_list(conn, None)?
        .into_iter()
        .filter(|p| parse_ts(&p.created_at).map(|t| t >= since_dt).unwrap_or(false))
        .count() as i64;

    // Automation/cost/learning from events.
    let remediations_auto = kpi_count(conn, M_REMEDIATION_AUTO, since)?;
    let remediations_approved = kpi_count(conn, M_REMEDIATION_APPROVED, since)?;
    let remediations_escalated = kpi_count(conn, M_REMEDIATION_ESCALATED, since)?;
    let remediation_total = remediations_auto + remediations_approved + remediations_escalated;
    let automation_ratio = ratio(remediations_auto, remediation_total);
    let autonomy_ratio = ratio(remediations_auto + remediations_approved, remediation_total);

    let promotions = kpi_count(conn, M_PROMOTION, since)?;
    let promotion_errors = kpi_count(conn, M_PROMOTION_ERROR, since)?;
    let promotion_error_rate = ratio(promotion_errors, promotions);

    Ok(KpiSummary {
        since: since.to_string(),
        incidents_opened,
        incidents_resolved,
        open_incidents,
        time_in_deviation_secs: deviation_secs,
        mttr_secs,
        recurring_problems,
        remediations_auto,
        remediations_approved,
        remediations_escalated,
        automation_ratio,
        autonomy_ratio,
        cost_llm_usd: kpi_sum(conn, M_COST_LLM, since)?,
        cost_avoided_usd: kpi_sum(conn, M_COST_AVOIDED, since)?,
        notice_filter_saved: kpi_count(conn, M_NOTICE_FILTER_SAVED, since)?,
        promotions,
        promotion_errors,
        promotion_error_rate,
    })
}

fn ratio(num: i64, den: i64) -> f64 {
    if den <= 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

/// The CSI constrained objective: minimize cost subject to reliability ≥ floor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Objective {
    /// Incident-resolution rate in the window (1.0 when nothing broke).
    pub reliability: f64,
    pub reliability_floor: f64,
    pub meets_floor: bool,
    /// The quantity we minimize while holding the constraint.
    pub cost_llm_usd: f64,
}

/// Evaluate the constrained objective for a summary against a reliability floor.
pub fn objective(s: &KpiSummary, reliability_floor: f64) -> Objective {
    let reliability = if s.incidents_opened == 0 {
        1.0
    } else {
        s.incidents_resolved as f64 / s.incidents_opened as f64
    };
    Objective {
        reliability,
        reliability_floor,
        meets_floor: reliability + f64::EPSILON >= reliability_floor,
        cost_llm_usd: s.cost_llm_usd,
    }
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
    fn events_record_sum_and_count() {
        let c = conn();
        let epoch = "1970-01-01T00:00:00Z";
        assert_eq!(kpi_sum(&c, M_COST_LLM, epoch).unwrap(), 0.0);
        kpi_record(&c, M_COST_LLM, 0.5, Some("chat")).unwrap();
        kpi_record(&c, M_COST_LLM, 1.25, None).unwrap();
        kpi_record(&c, M_NOTICE_FILTER_SAVED, 1.0, None).unwrap();
        assert!((kpi_sum(&c, M_COST_LLM, epoch).unwrap() - 1.75).abs() < 1e-9);
        assert_eq!(kpi_count(&c, M_COST_LLM, epoch).unwrap(), 2);
        assert_eq!(kpi_count(&c, M_NOTICE_FILTER_SAVED, epoch).unwrap(), 1);
        // future window sees nothing
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        assert_eq!(kpi_count(&c, M_COST_LLM, &future).unwrap(), 0);
    }

    #[test]
    fn reliability_and_automation_summary() {
        let c = conn();
        let epoch = "1970-01-01T00:00:00Z";

        // two incidents; resolve one
        let i1 = db::incident_open(&c, "a", "", "high", "monitor", Some("disk")).unwrap();
        let _i2 = db::incident_open(&c, "b", "", "low", "monitor", Some("net")).unwrap();
        db::incident_resolve(&c, &i1.id, "fixed").unwrap();

        // automation events
        kpi_record(&c, M_REMEDIATION_AUTO, 1.0, None).unwrap();
        kpi_record(&c, M_REMEDIATION_AUTO, 1.0, None).unwrap();
        kpi_record(&c, M_REMEDIATION_APPROVED, 1.0, None).unwrap();
        kpi_record(&c, M_REMEDIATION_ESCALATED, 1.0, None).unwrap();
        kpi_record(&c, M_COST_LLM, 2.0, None).unwrap();

        let s = summary(&c, epoch).unwrap();
        assert_eq!(s.incidents_opened, 2);
        assert_eq!(s.incidents_resolved, 1);
        assert_eq!(s.open_incidents, 1);
        assert!(s.mttr_secs.is_some());
        // auto / (auto+approved+escalated) = 2/4
        assert!((s.automation_ratio - 0.5).abs() < 1e-9);
        // (auto+approved)/total = 3/4
        assert!((s.autonomy_ratio - 0.75).abs() < 1e-9);
        assert!((s.cost_llm_usd - 2.0).abs() < 1e-9);

        let obj = objective(&s, 0.95);
        assert!((obj.reliability - 0.5).abs() < 1e-9);
        assert!(!obj.meets_floor, "0.5 resolution rate is below the 0.95 floor");
    }

    #[test]
    fn objective_passes_when_nothing_broke() {
        let c = conn();
        let s = summary(&c, "1970-01-01T00:00:00Z").unwrap();
        let obj = objective(&s, 0.95);
        assert_eq!(obj.reliability, 1.0);
        assert!(obj.meets_floor);
        assert_eq!(s.automation_ratio, 0.0, "no remediations yet");
        assert_eq!(s.promotion_error_rate, 0.0);
    }

    #[test]
    fn promotion_error_rate_computes() {
        let c = conn();
        let epoch = "1970-01-01T00:00:00Z";
        for _ in 0..4 {
            kpi_record(&c, M_PROMOTION, 1.0, None).unwrap();
        }
        kpi_record(&c, M_PROMOTION_ERROR, 1.0, None).unwrap();
        let s = summary(&c, epoch).unwrap();
        assert_eq!(s.promotions, 4);
        assert_eq!(s.promotion_errors, 1);
        assert!((s.promotion_error_rate - 0.25).abs() < 1e-9);
    }
}
