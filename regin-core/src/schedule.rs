//! Scheduling helpers (FEAT-047 / DISC-013).
//!
//! Cadence resolution and load-smoothing jitter for the scheduler. A skill's run
//! cadence comes from (highest precedence first): an explicit user/config
//! override, a per-domain tune in the to-be-state doc, then the skill's declared
//! default. Concurrent due-times are spread by a deterministic per-skill jitter so
//! many skills (and their LLM calls) never fire in the same instant.

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};

/// Parse a cadence string into a duration: `hourly|daily|weekly|monthly` or
/// `every <n>{s,m,h,d}`.
pub fn parse_interval(interval: &str) -> Result<Duration> {
    match interval {
        "hourly" => Ok(Duration::hours(1)),
        "daily" => Ok(Duration::days(1)),
        "weekly" => Ok(Duration::weeks(1)),
        "monthly" => Ok(Duration::days(30)),
        s if s.starts_with("every ") => {
            let spec = &s[6..];
            let unit = spec.chars().last().ok_or_else(|| anyhow!("Empty interval"))?;
            let num: i64 = spec[..spec.len() - 1].parse().context("Bad number")?;
            match unit {
                's' => Ok(Duration::seconds(num)),
                'm' => Ok(Duration::minutes(num)),
                'h' => Ok(Duration::hours(num)),
                'd' => Ok(Duration::days(num)),
                _ => Err(anyhow!("Unknown unit: {unit}")),
            }
        }
        _ => Err(anyhow!("Unknown interval: {interval}")),
    }
}

/// A stable per-seed fraction in `[0, 1)`, used to stagger skills deterministically
/// (so a given skill keeps a consistent offset, but different skills differ).
pub fn jitter_fraction(seed: &str) -> f64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut h);
    (h.finish() % 10_000) as f64 / 10_000.0
}

/// The next run time = `now + interval + jitter`, where jitter is up to
/// `max_fraction` of the interval, staggered deterministically by `skill`.
pub fn next_run_with_jitter(
    interval: &str,
    skill: &str,
    max_fraction: f64,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let base = parse_interval(interval)?;
    let max_fraction = max_fraction.clamp(0.0, 1.0);
    let jitter_secs = (base.num_seconds() as f64 * max_fraction * jitter_fraction(skill)) as i64;
    Ok(now + base + Duration::seconds(jitter_secs))
}

/// Resolve the effective cadence by precedence: explicit user/config override,
/// then the to-be-state per-domain tune, then the skill's declared default.
pub fn resolve_cadence(
    skill_default: Option<&str>,
    config_override: Option<&str>,
    desired_tune: Option<&str>,
) -> Option<String> {
    config_override
        .or(desired_tune)
        .or(skill_default)
        .map(|s| s.to_string())
}

/// Extract a skill's declared default cadence from its `skill.md` body: the first
/// `cadence: <value>` line (case-insensitive). Returns `None` if absent.
pub fn parse_skill_cadence(prompt: &str) -> Option<String> {
    for line in prompt.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("cadence:").or_else(|| t.strip_prefix("Cadence:")) {
            let v = rest.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_and_every_intervals() {
        assert_eq!(parse_interval("hourly").unwrap(), Duration::hours(1));
        assert_eq!(parse_interval("daily").unwrap(), Duration::days(1));
        assert_eq!(parse_interval("weekly").unwrap(), Duration::weeks(1));
        assert_eq!(parse_interval("monthly").unwrap(), Duration::days(30));
        assert_eq!(parse_interval("every 15m").unwrap(), Duration::minutes(15));
        assert_eq!(parse_interval("every 2h").unwrap(), Duration::hours(2));
        assert!(parse_interval("fortnightly").is_err());
        assert!(parse_interval("every 5x").is_err());
    }

    #[test]
    fn jitter_is_stable_per_seed_and_bounded() {
        let a = jitter_fraction("disk");
        assert_eq!(a, jitter_fraction("disk"), "stable for the same seed");
        assert!((0.0..1.0).contains(&a));
        // different seeds generally differ
        assert_ne!(jitter_fraction("disk"), jitter_fraction("network"));
    }

    #[test]
    fn next_run_falls_within_base_and_base_plus_jitter() {
        let now = DateTime::parse_from_rfc3339("2026-06-19T00:00:00Z").unwrap().with_timezone(&Utc);
        let nr = next_run_with_jitter("hourly", "disk", 0.1, now).unwrap();
        let base = now + Duration::hours(1);
        let max = base + Duration::seconds((3600.0 * 0.1) as i64);
        assert!(nr >= base && nr <= max, "next run within [base, base+10%]");
        // zero jitter -> exactly base
        assert_eq!(next_run_with_jitter("hourly", "disk", 0.0, now).unwrap(), base);
    }

    #[test]
    fn cadence_precedence_override_then_tune_then_default() {
        assert_eq!(resolve_cadence(Some("daily"), Some("hourly"), Some("weekly")).as_deref(), Some("hourly"));
        assert_eq!(resolve_cadence(Some("daily"), None, Some("weekly")).as_deref(), Some("weekly"));
        assert_eq!(resolve_cadence(Some("daily"), None, None).as_deref(), Some("daily"));
        assert_eq!(resolve_cadence(None, None, None), None);
    }

    #[test]
    fn skill_cadence_line_is_parsed() {
        let md = "disk-usage: check disk\n\ncadence: every 15m\n\nDo the thing.\n";
        assert_eq!(parse_skill_cadence(md).as_deref(), Some("every 15m"));
        assert_eq!(parse_skill_cadence("no cadence here\njust text").as_deref(), None);
    }
}
