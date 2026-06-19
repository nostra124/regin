//! Adaptive autonomy posture (FEAT-040 / DISC-009).
//!
//! regin starts **conservative** — most fixes route to `pending_approval` even
//! when safe — and *earns* auto-apply on evidence: the safe lane graduates once
//! the change-success rate clears a floor over a minimum sample, and the
//! promotion-error rate stays low. Graduation is reversible: a failure/error
//! spike demotes it back to conservative. A master switch bounds the whole thing.
//!
//! This is the same earn-trust-with-evidence pattern as the DISC-015 promotion
//! loop, governed by the shared KPI store (FEAT-050).

use serde::{Deserialize, Serialize};

use crate::kpi::KpiSummary;

/// The current autonomy posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Posture {
    /// Safe fixes still route to approval until trust is earned.
    Conservative,
    /// Safe, reversible fixes may auto-apply.
    Trusted,
}

impl std::fmt::Display for Posture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Posture::Conservative => write!(f, "conservative"),
            Posture::Trusted => write!(f, "trusted"),
        }
    }
}

impl Posture {
    /// Whether this posture permits auto-applying a (safe, reversible) fix.
    pub fn permits_auto(&self) -> bool {
        matches!(self, Posture::Trusted)
    }
}

/// Tunables for [`compute`]. `allow_auto` is the master bound (DISC-009): when
/// false, posture is forced conservative regardless of evidence.
#[derive(Debug, Clone, Copy)]
pub struct PosturePolicy {
    pub allow_auto: bool,
    pub min_samples: i64,
    pub min_success_rate: f64,
    pub max_promotion_error_rate: f64,
}

impl Default for PosturePolicy {
    fn default() -> Self {
        Self {
            allow_auto: true,
            min_samples: 10,
            min_success_rate: 0.9,
            max_promotion_error_rate: 0.1,
        }
    }
}

/// Compute the posture from the KPI snapshot and policy. Trusted requires: the
/// master switch on, enough change samples, a high-enough success rate, and a
/// low-enough promotion-error rate. Anything short stays/demotes to conservative.
pub fn compute(summary: &KpiSummary, policy: PosturePolicy) -> Posture {
    if !policy.allow_auto {
        return Posture::Conservative;
    }
    let samples = summary.change_successes + summary.change_failures;
    let enough = samples >= policy.min_samples;
    let reliable = summary.change_success_rate >= policy.min_success_rate;
    // A promotion-error spike demotes (only meaningful once promotions exist).
    let promotion_ok = summary.promotions == 0
        || summary.promotion_error_rate <= policy.max_promotion_error_rate;
    if enough && reliable && promotion_ok {
        Posture::Trusted
    } else {
        Posture::Conservative
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(successes: i64, failures: i64, promotions: i64, promo_errors: i64) -> KpiSummary {
        let total = successes + failures;
        let prate = if promotions == 0 { 0.0 } else { promo_errors as f64 / promotions as f64 };
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
            promotions,
            promotion_errors: promo_errors,
            promotion_error_rate: prate,
            change_successes: successes,
            change_failures: failures,
            change_success_rate: if total == 0 { 0.0 } else { successes as f64 / total as f64 },
        }
    }

    #[test]
    fn starts_conservative_without_evidence() {
        assert_eq!(compute(&summary(0, 0, 0, 0), PosturePolicy::default()), Posture::Conservative);
        // a few successes but below the sample floor
        assert_eq!(compute(&summary(3, 0, 0, 0), PosturePolicy::default()), Posture::Conservative);
    }

    #[test]
    fn graduates_with_enough_clean_evidence() {
        // 12 successes, 0 failures -> 100% over >= 10 samples
        assert_eq!(compute(&summary(12, 0, 0, 0), PosturePolicy::default()), Posture::Trusted);
        assert!(compute(&summary(12, 0, 0, 0), PosturePolicy::default()).permits_auto());
    }

    #[test]
    fn demotes_on_failure_spike() {
        // 10 successes, 5 failures -> 0.66 success rate, below 0.9
        assert_eq!(compute(&summary(10, 5, 0, 0), PosturePolicy::default()), Posture::Conservative);
    }

    #[test]
    fn demotes_on_promotion_error_spike() {
        // good change record, but promotion errors high
        let s = summary(20, 0, 10, 5); // promotion_error_rate 0.5 > 0.1
        assert_eq!(compute(&s, PosturePolicy::default()), Posture::Conservative);
    }

    #[test]
    fn master_switch_forces_conservative() {
        let policy = PosturePolicy { allow_auto: false, ..PosturePolicy::default() };
        assert_eq!(compute(&summary(100, 0, 0, 0), policy), Posture::Conservative);
    }

    #[test]
    fn posture_permits_auto_only_when_trusted() {
        assert!(Posture::Trusted.permits_auto());
        assert!(!Posture::Conservative.permits_auto());
    }
}
