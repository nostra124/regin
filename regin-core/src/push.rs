//! Critical-only active push (FEAT-044 / DISC-010).
//!
//! An **opt-in, off-by-default**, severity-gated egress so a genuine emergency
//! reaches the operator actively instead of waiting for the next login greeting
//! (FEAT-043). Only items at/above the configured severity are pushed; everything
//! else still waits for the greeting. A push failure is non-fatal — the item is
//! already parked, so it simply surfaces at login (never lost).
//!
//! The gating + rate-limit decisions are pure and unit-tested; the channel send
//! (ntfy / webhook) is thin I/O.

use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Severity ladder. Declaration order is the ordering (Low < … < Critical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Parse a severity string; unknown values map to `Low` so they never
    /// accidentally trip the critical-only gate.
    pub fn parse(s: &str) -> Severity {
        match s.trim().to_lowercase().as_str() {
            "critical" | "crit" => Severity::Critical,
            "high" => Severity::High,
            "medium" | "med" => Severity::Medium,
            _ => Severity::Low,
        }
    }
}

/// The push policy from config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushPolicy {
    pub enabled: bool,
    pub min_severity: Severity,
}

/// Whether an item of `severity` should be actively pushed: the channel must be
/// enabled and the severity at/above the gate. Off by default.
pub fn should_push(policy: PushPolicy, severity: Severity) -> bool {
    policy.enabled && severity >= policy.min_severity
}

/// Rate-limit / dedup: allow a send only if none happened within `min_interval`.
pub fn rate_limit_ok(last_sent: Option<DateTime<Utc>>, now: DateTime<Utc>, min_interval: Duration) -> bool {
    match last_sent {
        None => true,
        Some(t) => now - t >= min_interval,
    }
}

/// The configured push channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    None,
    /// ntfy.sh-style topic: POST the body to the target URL.
    Ntfy,
    /// Generic webhook: POST a JSON `{title, body}`.
    Webhook,
}

impl Channel {
    pub fn parse(s: &str) -> Channel {
        match s.trim().to_lowercase().as_str() {
            "ntfy" => Channel::Ntfy,
            "webhook" => Channel::Webhook,
            _ => Channel::None,
        }
    }
}

/// Send a notification over the configured channel. Errors are returned so the
/// caller can fall back to the parked/greeting path (the item is never lost).
pub async fn send(channel: Channel, target: &str, title: &str, body: &str) -> Result<()> {
    if target.is_empty() {
        return Err(anyhow!("no push target configured"));
    }
    let client = reqwest::Client::new();
    match channel {
        Channel::None => Err(anyhow!("push channel is not configured")),
        Channel::Ntfy => {
            client
                .post(target)
                .header("Title", title)
                .body(body.to_string())
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        Channel::Webhook => {
            client
                .post(target)
                .json(&serde_json::json!({ "title": title, "body": body }))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_parses_and_orders() {
        assert_eq!(Severity::parse("critical"), Severity::Critical);
        assert_eq!(Severity::parse("HIGH"), Severity::High);
        assert_eq!(Severity::parse("nonsense"), Severity::Low);
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
    }

    #[test]
    fn off_by_default_and_severity_gated() {
        // disabled -> never pushes, even a critical
        let off = PushPolicy { enabled: false, min_severity: Severity::Critical };
        assert!(!should_push(off, Severity::Critical));

        // enabled, critical gate -> only critical passes
        let on = PushPolicy { enabled: true, min_severity: Severity::Critical };
        assert!(should_push(on, Severity::Critical));
        assert!(!should_push(on, Severity::High));
        assert!(!should_push(on, Severity::Low));

        // a lower gate lets high through too
        let high = PushPolicy { enabled: true, min_severity: Severity::High };
        assert!(should_push(high, Severity::Critical));
        assert!(should_push(high, Severity::High));
        assert!(!should_push(high, Severity::Medium));
    }

    #[test]
    fn rate_limit_blocks_repeats_within_window() {
        let now = Utc::now();
        assert!(rate_limit_ok(None, now, Duration::seconds(300)), "first send allowed");
        assert!(!rate_limit_ok(Some(now - Duration::seconds(60)), now, Duration::seconds(300)), "too soon");
        assert!(rate_limit_ok(Some(now - Duration::seconds(301)), now, Duration::seconds(300)), "window elapsed");
    }

    #[test]
    fn channel_parses() {
        assert_eq!(Channel::parse("ntfy"), Channel::Ntfy);
        assert_eq!(Channel::parse("webhook"), Channel::Webhook);
        assert_eq!(Channel::parse("email"), Channel::None);
    }

    #[tokio::test]
    async fn send_requires_target_and_channel() {
        assert!(send(Channel::Ntfy, "", "t", "b").await.is_err(), "empty target errors");
        assert!(send(Channel::None, "http://x", "t", "b").await.is_err(), "no channel errors");
    }
}
