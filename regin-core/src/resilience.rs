//! Operator resilience primitives (FEAT-048 / DISC-013).
//!
//! regin runs unattended 24/7 against an external LLM. These primitives keep it
//! alive and honest:
//! - **exponential backoff** for the LLM API so a transient outage isn't hammered;
//! - an **outage tracker** that, past a threshold, signals a self-incident while
//!   monitoring degrades to the LLM-free deterministic checks (FEAT-051, run by
//!   the two-tier engine FEAT-049);
//! - a **heartbeat staleness** check so a stalled scheduler is detectable atop the
//!   systemd process watchdog.
//!
//! Downtime recovery is *coalesced by construction*: each schedule carries a
//! single `next_run`, so on restart a due skill runs **once**, never once per
//! missed interval.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::time::Duration;

/// Exponential backoff with a cap: `base * 2^attempt`, clamped to `max`.
/// `attempt` is 0-based (attempt 0 waits `base`).
pub fn backoff_delay(attempt: u32, base: Duration, max: Duration) -> Duration {
    let factor = 2u64.saturating_pow(attempt);
    let secs = base.as_secs().saturating_mul(factor);
    Duration::from_secs(secs.min(max.as_secs()))
}

/// Tracks an ongoing dependency (LLM) outage to decide when to raise a
/// self-incident and whether to run in degraded (LLM-free) mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutageTracker {
    pub consecutive_failures: u32,
    pub first_failure_at: Option<DateTime<Utc>>,
}

impl OutageTracker {
    /// Record a failed interaction (stamps the outage start on the first failure).
    pub fn record_failure(&mut self, now: DateTime<Utc>) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.first_failure_at.is_none() {
            self.first_failure_at = Some(now);
        }
    }

    /// Record a success — clears the outage.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.first_failure_at = None;
    }

    /// Whether currently in an outage.
    pub fn in_outage(&self) -> bool {
        self.consecutive_failures > 0
    }

    /// How long the current outage has lasted, if any.
    pub fn outage_duration(&self, now: DateTime<Utc>) -> Option<ChronoDuration> {
        self.first_failure_at.map(|t| now - t)
    }

    /// Whether the outage has lasted long enough to raise a self-incident.
    pub fn should_raise_incident(&self, now: DateTime<Utc>, threshold: ChronoDuration) -> bool {
        self.outage_duration(now).map(|d| d >= threshold).unwrap_or(false)
    }
}

/// Whether the scheduler heartbeat is stale (no tick within `max_gap`) — a sign
/// the loop has stalled, even though the systemd service still runs.
pub fn heartbeat_stale(last: DateTime<Utc>, now: DateTime<Utc>, max_gap: ChronoDuration) -> bool {
    now - last > max_gap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_caps() {
        let base = Duration::from_secs(2);
        let max = Duration::from_secs(60);
        assert_eq!(backoff_delay(0, base, max), Duration::from_secs(2));
        assert_eq!(backoff_delay(1, base, max), Duration::from_secs(4));
        assert_eq!(backoff_delay(2, base, max), Duration::from_secs(8));
        assert_eq!(backoff_delay(3, base, max), Duration::from_secs(16));
        assert_eq!(backoff_delay(10, base, max), Duration::from_secs(60), "capped at max");
        // huge attempt does not overflow
        assert_eq!(backoff_delay(64, base, max), Duration::from_secs(60));
    }

    #[test]
    fn outage_tracker_lifecycle() {
        let t0 = DateTime::parse_from_rfc3339("2026-06-19T00:00:00Z").unwrap().with_timezone(&Utc);
        let mut o = OutageTracker::default();
        assert!(!o.in_outage());
        assert!(o.outage_duration(t0).is_none());

        o.record_failure(t0);
        assert!(o.in_outage());
        assert_eq!(o.consecutive_failures, 1);
        let later = t0 + ChronoDuration::seconds(120);
        o.record_failure(later); // start time stays at first failure
        assert_eq!(o.consecutive_failures, 2);
        assert_eq!(o.outage_duration(later).unwrap(), ChronoDuration::seconds(120));

        // threshold: 120s outage meets a 60s threshold, not a 300s one
        assert!(o.should_raise_incident(later, ChronoDuration::seconds(60)));
        assert!(!o.should_raise_incident(later, ChronoDuration::seconds(300)));

        o.record_success();
        assert!(!o.in_outage());
        assert!(o.first_failure_at.is_none());
        assert!(!o.should_raise_incident(later, ChronoDuration::seconds(1)));
    }

    #[test]
    fn heartbeat_staleness() {
        let t0 = DateTime::parse_from_rfc3339("2026-06-19T00:00:00Z").unwrap().with_timezone(&Utc);
        assert!(!heartbeat_stale(t0, t0 + ChronoDuration::seconds(30), ChronoDuration::seconds(60)));
        assert!(heartbeat_stale(t0, t0 + ChronoDuration::seconds(61), ChronoDuration::seconds(60)));
    }
}
