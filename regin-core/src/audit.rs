//! Scheduled operator self-audit (FEAT-055 / DISC-016).
//!
//! A periodic wide-lens CSI sweep so the continuous loop doesn't drift: it reviews
//! the KPIs (FEAT-050), checks monitoring **coverage** (domains with a to-be-state
//! but no operator skill, or vice versa), and flags promotion-error pressure
//! (FEAT-051). Findings are filed as **problems** for human review; any to-be-state
//! edit it would propose always routes through approval (never a silent rewrite).
//!
//! The sweep is **budgeted**: when over budget it trims scope (skips the coverage
//! walk) and records that it trimmed. Cadence is adaptive — monthly by default,
//! more often while KPIs are volatile.

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::kpi::{self, KpiSummary};

/// One audit finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub area: String,
    pub message: String,
}

/// The result of an audit sweep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditReport {
    pub findings: Vec<Finding>,
    /// Whether scope was trimmed to stay within budget.
    pub trimmed: bool,
}

impl AuditReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Review KPIs for regressions (reliability below floor, promotion-error
/// pressure, runaway open incidents).
pub fn audit_kpis(summary: &KpiSummary, reliability_floor: f64) -> Vec<Finding> {
    let mut out = Vec::new();
    let obj = kpi::objective(summary, reliability_floor);
    if !obj.meets_floor {
        out.push(Finding {
            area: "reliability".into(),
            message: format!(
                "incident-resolution rate {:.0}% is below the {:.0}% floor",
                obj.reliability * 100.0,
                reliability_floor * 100.0
            ),
        });
    }
    if summary.promotions > 0 && summary.promotion_error_rate > 0.1 {
        out.push(Finding {
            area: "promotion".into(),
            message: format!(
                "promotion-error rate {:.0}% — tighten promotion criteria",
                summary.promotion_error_rate * 100.0
            ),
        });
    }
    if summary.open_incidents > 0 && summary.change_success_rate < 0.5 && summary.change_failures > 0 {
        out.push(Finding {
            area: "remediation".into(),
            message: "change-success rate is low while incidents are open — review the playbooks".into(),
        });
    }
    out
}

/// Coverage gaps: a domain with a to-be-state but no operator skill (unmonitored
/// intent), or an operator skill with no to-be-state (nothing to judge against).
pub fn audit_coverage(skill_domains: &[String], desired_domains: &[String]) -> Vec<Finding> {
    let mut out = Vec::new();
    for d in desired_domains {
        if !skill_domains.contains(d) {
            out.push(Finding {
                area: "coverage".into(),
                message: format!("domain `{d}` has a to-be-state but no operator skill monitoring it"),
            });
        }
    }
    for s in skill_domains {
        if !desired_domains.contains(s) {
            out.push(Finding {
                area: "coverage".into(),
                message: format!("operator skill `{s}` has no to-be-state to judge against"),
            });
        }
    }
    out
}

/// Run the sweep. When `over_budget`, the coverage walk is skipped (trimmed) — the
/// cheap KPI review still runs.
pub fn run_audit(
    summary: &KpiSummary,
    reliability_floor: f64,
    skill_domains: &[String],
    desired_domains: &[String],
    over_budget: bool,
) -> AuditReport {
    let mut findings = audit_kpis(summary, reliability_floor);
    let trimmed = over_budget;
    if !trimmed {
        findings.extend(audit_coverage(skill_domains, desired_domains));
    }
    AuditReport { findings, trimmed }
}

/// File the report's findings as problems for human review (idempotent by title),
/// and record the audit-run KPI. Returns how many new problems were opened.
pub fn file_findings(conn: &Connection, report: &AuditReport) -> Result<usize> {
    let open = db::problem_list(conn, Some("open"))?;
    let mut opened = 0;
    for f in &report.findings {
        let title = format!("self-audit [{}]: {}", f.area, f.message);
        if !open.iter().any(|p| p.title == title) {
            db::problem_open(conn, &title, "Raised by the periodic operator self-audit (FEAT-055).")?;
            opened += 1;
        }
    }
    kpi::kpi_record(conn, "audit.run", 1.0, if report.trimmed { Some("trimmed") } else { None })?;
    Ok(opened)
}

/// Adaptive cadence in days: shorter while volatile (young / shifting KPIs),
/// monthly once stable.
pub fn adaptive_cadence_days(volatile: bool) -> i64 {
    if volatile { 7 } else { 30 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> KpiSummary {
        KpiSummary {
            since: "1970-01-01T00:00:00Z".into(),
            incidents_opened: 0,
            incidents_resolved: 0,
            open_incidents: 0,
            time_in_deviation_secs: 0,
            mttr_secs: None,
            recurring_problems: 0,
            remediations_auto: 0,
            remediations_approved: 0,
            remediations_escalated: 0,
            automation_ratio: 0.0,
            autonomy_ratio: 0.0,
            cost_llm_usd: 0.0,
            cost_avoided_usd: 0.0,
            notice_filter_saved: 0,
            promotions: 0,
            promotion_errors: 0,
            promotion_error_rate: 0.0,
            change_successes: 0,
            change_failures: 0,
            change_success_rate: 0.0,
        }
    }

    #[test]
    fn clean_kpis_yield_no_findings() {
        // nothing broke -> reliability 1.0, no promotions -> no findings
        assert!(audit_kpis(&summary(), 0.95).is_empty());
    }

    #[test]
    fn kpi_regressions_are_flagged() {
        let mut s = summary();
        s.incidents_opened = 10;
        s.incidents_resolved = 5; // 0.5 < 0.95 floor
        s.promotions = 10;
        s.promotion_errors = 3; // 0.3 > 0.1
        s.promotion_error_rate = 0.3;
        let f = audit_kpis(&s, 0.95);
        assert!(f.iter().any(|f| f.area == "reliability"));
        assert!(f.iter().any(|f| f.area == "promotion"));
    }

    #[test]
    fn coverage_gaps_both_directions() {
        let skills = vec!["disk".to_string(), "extra".to_string()];
        let desired = vec!["disk".to_string(), "lonely".to_string()];
        let f = audit_coverage(&skills, &desired);
        assert!(f.iter().any(|f| f.message.contains("`lonely`") && f.message.contains("no operator skill")));
        assert!(f.iter().any(|f| f.message.contains("`extra`") && f.message.contains("no to-be-state")));
        // fully covered -> none
        assert!(audit_coverage(&["disk".into()], &["disk".into()]).is_empty());
    }

    #[test]
    fn over_budget_trims_coverage_but_keeps_kpis() {
        let mut s = summary();
        s.incidents_opened = 4;
        s.incidents_resolved = 1; // reliability finding
        let skills = vec!["a".to_string()];
        let desired: Vec<String> = vec![]; // would yield a coverage finding
        let report = run_audit(&s, 0.95, &skills, &desired, true);
        assert!(report.trimmed);
        assert!(report.findings.iter().all(|f| f.area != "coverage"), "coverage walk skipped under budget");
        assert!(report.findings.iter().any(|f| f.area == "reliability"), "cheap KPI review still runs");
    }

    #[test]
    fn file_findings_is_idempotent_and_records_kpi() {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        let report = AuditReport {
            findings: vec![Finding { area: "coverage".into(), message: "domain `x` has a to-be-state but no operator skill monitoring it".into() }],
            trimmed: false,
        };
        assert_eq!(file_findings(&c, &report).unwrap(), 1);
        assert_eq!(file_findings(&c, &report).unwrap(), 0, "same finding not re-opened");
        assert_eq!(db::problem_list(&c, None).unwrap().len(), 1);
        assert_eq!(kpi::kpi_count(&c, "audit.run", "1970-01-01T00:00:00Z").unwrap(), 2);
    }

    #[test]
    fn cadence_is_adaptive() {
        assert_eq!(adaptive_cadence_days(true), 7);
        assert_eq!(adaptive_cadence_days(false), 30);
    }
}
