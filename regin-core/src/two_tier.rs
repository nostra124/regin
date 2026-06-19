//! Two-tier evaluation engine (FEAT-049 / DISC-015).
//!
//! Monitoring runs in two tiers that both feed the incident flow:
//! - a **cheap deterministic tier** — structured to-be-state assertions
//!   (FEAT-034) plus promoted checks (FEAT-051), run frequently and LLM-free;
//! - a **periodic LLM review tier** — judges unstructured/novel signals, after
//!   notice filters (FEAT-052) have dropped known noise to cut cost.
//!
//! The deterministic tier is also the **degraded-mode fallback**: when the LLM is
//! unavailable (FEAT-048), monitoring keeps running on it alone. The baseline
//! "senseful full automation" directive lives in
//! [`crate::context::OPERATOR_DIRECTIVE`].

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::desired::{AssertValue, DesiredState};
use crate::evaluate::{self, Deviation};
use crate::filters::{self, FilterRule};

/// Outcome of a deterministic-tier pass over one domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierReport {
    pub domain: String,
    /// Whether the LLM tier was skipped (degraded mode).
    pub degraded: bool,
    pub deviations: Vec<Deviation>,
    /// The incident opened/updated for the deviations, if any.
    pub incident_id: Option<String>,
}

/// Drop known-noise lines via notice filters before the LLM tier, recording the
/// notice-filter-savings KPI for each drop (FEAT-052). Returns the survivors that
/// would proceed to LLM review.
pub fn prefilter_unstructured(
    conn: &Connection,
    rules: &[FilterRule],
    domain: &str,
    lines: &[String],
) -> Result<Vec<String>> {
    let mut survivors = Vec::new();
    for line in lines {
        if !filters::filter_and_record(conn, rules, domain, line)? {
            survivors.push(line.clone());
        }
    }
    Ok(survivors)
}

/// Run the cheap deterministic tier for a domain: evaluate observed structured
/// signals against the to-be-state assertions and raise a deduped incident for
/// any deviation. This is what keeps monitoring alive in degraded mode.
pub fn run_deterministic(
    conn: &Connection,
    ds: &DesiredState,
    structured: &BTreeMap<String, AssertValue>,
    severity: &str,
    degraded: bool,
) -> Result<TierReport> {
    let deviations = evaluate::evaluate(ds, structured);
    let incident_id = evaluate::raise_for_deviations(conn, &ds.domain, &deviations, severity)?;
    Ok(TierReport {
        domain: ds.domain.clone(),
        degraded,
        deviations,
        incident_id,
    })
}

/// One full evaluation pass for a domain. The deterministic tier always runs; the
/// unstructured stream is pre-filtered for the LLM tier (which the caller invokes
/// with a [`crate::evaluate::DeviationJudge`] when the LLM is available). When
/// `llm_available` is false the pass is degraded — deterministic only.
pub fn evaluate_pass(
    conn: &Connection,
    ds: &DesiredState,
    structured: &BTreeMap<String, AssertValue>,
    unstructured: &[String],
    rules: &[FilterRule],
    severity: &str,
    llm_available: bool,
) -> Result<(TierReport, Vec<String>)> {
    let report = run_deterministic(conn, ds, structured, severity, !llm_available)?;
    // Only bother pre-filtering for the LLM tier when the LLM can actually run.
    let survivors = if llm_available {
        prefilter_unstructured(conn, rules, &ds.domain, unstructured)?
    } else {
        Vec::new()
    };
    Ok((report, survivors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::desired::{parse_desired_state, DesiredSource};
    use crate::filters::FilterSource;
    use crate::kpi;
    use std::path::PathBuf;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn disk_ds() -> DesiredState {
        let md = "# disk\n\n```assertions\n[[assert]]\nkey=\"disk.root.use_percent\"\nop=\"lt\"\nvalue=90\n```\n";
        parse_desired_state("disk", md, DesiredSource::System, PathBuf::new()).unwrap()
    }

    #[test]
    fn deterministic_tier_raises_on_breach() {
        let c = conn();
        let ds = disk_ds();
        let mut obs = BTreeMap::new();
        obs.insert("disk.root.use_percent".to_string(), AssertValue::Num(70.0));
        let ok = run_deterministic(&c, &ds, &obs, "high", false).unwrap();
        assert!(ok.deviations.is_empty());
        assert!(ok.incident_id.is_none());

        obs.insert("disk.root.use_percent".to_string(), AssertValue::Num(95.0));
        let bad = run_deterministic(&c, &ds, &obs, "high", true).unwrap();
        assert_eq!(bad.deviations.len(), 1);
        assert!(bad.incident_id.is_some());
        assert!(bad.degraded, "flagged as degraded when llm unavailable");
    }

    #[test]
    fn prefilter_drops_noise_and_counts_kpi() {
        let c = conn();
        let rules = filters::parse_rules("[[rule]]\nname=\"dbg\"\ncontains=\"DEBUG\"\n", FilterSource::System).unwrap();
        let lines = vec!["DEBUG chatter".to_string(), "real warning".to_string()];
        let survivors = prefilter_unstructured(&c, &rules, "logs", &lines).unwrap();
        assert_eq!(survivors, vec!["real warning".to_string()]);
        assert_eq!(kpi::kpi_count(&c, kpi::M_NOTICE_FILTER_SAVED, "1970-01-01T00:00:00Z").unwrap(), 1);
    }

    #[test]
    fn degraded_pass_skips_llm_prefilter() {
        let c = conn();
        let ds = disk_ds();
        let rules = filters::parse_rules("[[rule]]\nname=\"dbg\"\ncontains=\"DEBUG\"\n", FilterSource::System).unwrap();
        let obs = BTreeMap::new();
        let unstructured = vec!["DEBUG x".to_string(), "y".to_string()];

        // degraded: deterministic only, no prefilter work, no survivors
        let (report, survivors) = evaluate_pass(&c, &ds, &obs, &unstructured, &rules, "high", false).unwrap();
        assert!(report.degraded);
        assert!(survivors.is_empty());
        assert_eq!(kpi::kpi_count(&c, kpi::M_NOTICE_FILTER_SAVED, "1970-01-01T00:00:00Z").unwrap(), 0);

        // llm available: prefilter runs, noise dropped, one survivor
        let (report, survivors) = evaluate_pass(&c, &ds, &obs, &unstructured, &rules, "high", true).unwrap();
        assert!(!report.degraded);
        assert_eq!(survivors, vec!["y".to_string()]);
    }
}
