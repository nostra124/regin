//! Effective operating-mode detection (FEAT-041 / DISC-010).
//!
//! regin escalates differently depending on whether it is effectively part of an
//! org (a supervisor reachable over the bus) or running standalone. Per DISC-010
//! Variant C the **effective mode = configured target AND recent reachability**:
//! a configured-but-unreachable bus falls back to standalone, and a single
//! transient blip does not flip the mode (it is debounced on consecutive
//! failures and a staleness grace window).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// regin's effective operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// A supervisor bus is configured and recently reachable.
    Org,
    /// No usable supervisor — park/greet locally.
    Standalone,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Org => write!(f, "org"),
            Mode::Standalone => write!(f, "standalone"),
        }
    }
}

/// Rolling reachability health for the bus.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReachabilityState {
    /// Last successful bus interaction, if any.
    pub last_ok: Option<DateTime<Utc>>,
    /// Consecutive failures since the last success.
    pub consecutive_failures: u32,
}

impl ReachabilityState {
    /// A successful bus round-trip: clears the failure streak and stamps now.
    pub fn record_success(&mut self, now: DateTime<Utc>) {
        self.last_ok = Some(now);
        self.consecutive_failures = 0;
    }

    /// A failed bus interaction: extends the failure streak.
    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }
}

/// Tunables for [`effective_mode`].
#[derive(Debug, Clone, Copy)]
pub struct ModePolicy {
    /// How long a last success stays "recent".
    pub grace: Duration,
    /// Consecutive failures that flip a configured bus to standalone (debounce).
    pub failure_threshold: u32,
}

impl Default for ModePolicy {
    fn default() -> Self {
        Self {
            grace: Duration::seconds(300),
            failure_threshold: 3,
        }
    }
}

/// Compute the effective mode (Variant C). `configured` is whether a supervisor
/// bus/persona is configured at all.
pub fn effective_mode(
    configured: bool,
    reach: &ReachabilityState,
    now: DateTime<Utc>,
    policy: ModePolicy,
) -> Mode {
    if !configured {
        return Mode::Standalone;
    }
    // A sustained failure streak flips to standalone; a transient one does not.
    if reach.consecutive_failures >= policy.failure_threshold {
        return Mode::Standalone;
    }
    match reach.last_ok {
        Some(t) if now - t <= policy.grace => Mode::Org,
        // Stale or never reached -> not effectively in an org yet.
        _ => Mode::Standalone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-19T12:00:00Z").unwrap().with_timezone(&Utc)
    }

    #[test]
    fn unconfigured_is_always_standalone() {
        let r = ReachabilityState::default();
        assert_eq!(effective_mode(false, &r, t0(), ModePolicy::default()), Mode::Standalone);
    }

    #[test]
    fn configured_and_fresh_is_org() {
        let mut r = ReachabilityState::default();
        r.record_success(t0());
        assert_eq!(effective_mode(true, &r, t0(), ModePolicy::default()), Mode::Org);
    }

    #[test]
    fn never_reached_is_standalone_even_if_configured() {
        let r = ReachabilityState::default();
        assert_eq!(effective_mode(true, &r, t0(), ModePolicy::default()), Mode::Standalone);
    }

    #[test]
    fn single_transient_failure_does_not_flip() {
        let mut r = ReachabilityState::default();
        r.record_success(t0());
        r.record_failure(); // one blip, below threshold 3
        assert_eq!(r.consecutive_failures, 1);
        assert_eq!(effective_mode(true, &r, t0(), ModePolicy::default()), Mode::Org);
    }

    #[test]
    fn sustained_failures_flip_to_standalone() {
        let mut r = ReachabilityState::default();
        r.record_success(t0());
        for _ in 0..3 {
            r.record_failure();
        }
        assert_eq!(effective_mode(true, &r, t0(), ModePolicy::default()), Mode::Standalone);
    }

    #[test]
    fn stale_success_beyond_grace_is_standalone() {
        let mut r = ReachabilityState::default();
        r.record_success(t0());
        let later = t0() + Duration::seconds(301);
        assert_eq!(effective_mode(true, &r, later, ModePolicy::default()), Mode::Standalone);
        // and a success at the later time restores org
        r.record_success(later);
        assert_eq!(effective_mode(true, &r, later, ModePolicy::default()), Mode::Org);
    }

    #[test]
    fn success_clears_failure_streak() {
        let mut r = ReachabilityState::default();
        r.record_failure();
        r.record_failure();
        r.record_success(t0());
        assert_eq!(r.consecutive_failures, 0);
        assert!(r.last_ok.is_some());
    }
}
