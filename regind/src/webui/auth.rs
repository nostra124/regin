//! Web UI authentication (FEAT-087, acceptance criterion 5): PAM login,
//! bearer tokens, and a per-IP rate limiter. Token storage and the rate
//! limiter are pure/DB logic, unit-tested directly; [`super::pam_auth`] is
//! the one real-I/O piece (a real `libpam` call), tested separately against
//! `pam_permit`/`pam_deny` throwaway service files.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// The PAM service name used for web UI logins (acceptance criterion 5).
/// The shipped package installs `/etc/pam.d/regin` (criterion 12).
pub const PAM_SERVICE: &str = "regin";

/// Bearer tokens are valid for this long after issue (criterion 5).
pub const TOKEN_TTL: Duration = Duration::hours(24);

pub fn ensure_webui_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS webui_tokens (
            token_hash TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS webui_tabs (
            name TEXT PRIMARY KEY,
            icon TEXT NOT NULL,
            html TEXT NOT NULL,
            created_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// A fresh 32-byte random token, hex-encoded (acceptance criterion 5).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// SHA-256 hex digest — only the hash is ever stored, never the raw token
/// (acceptance criterion 5).
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Issue a new bearer token for `username`, valid for [`TOKEN_TTL`]. Returns
/// the raw token (given to the client once, never stored).
pub fn issue_token(conn: &Connection, username: &str, now: DateTime<Utc>) -> Result<String> {
    let token = generate_token();
    let hash = hash_token(&token);
    let expires = now + TOKEN_TTL;
    conn.execute(
        "INSERT OR REPLACE INTO webui_tokens (token_hash, username, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![hash, username, now.to_rfc3339(), expires.to_rfc3339()],
    )?;
    Ok(token)
}

/// Validate a bearer token: `Some(username)` if it exists and hasn't
/// expired, `None` otherwise (unknown token, or expired — the two look the
/// same to a caller, deliberately, to avoid leaking which case it was).
pub fn validate_token(conn: &Connection, token: &str, now: DateTime<Utc>) -> Result<Option<String>> {
    let hash = hash_token(token);
    let row: Option<(String, String)> =
        conn.query_row("SELECT username, expires_at FROM webui_tokens WHERE token_hash = ?1", params![hash], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
    let Some((username, expires_at)) = row else {
        return Ok(None);
    };
    let expires: DateTime<Utc> = expires_at.parse()?;
    Ok(if now < expires { Some(username) } else { None })
}

/// Revoke a token outright (used by `/auth/refresh`, criterion 5: "issues a
/// new token, old one revoked").
pub fn revoke_token(conn: &Connection, token: &str) -> Result<()> {
    let hash = hash_token(token);
    conn.execute("DELETE FROM webui_tokens WHERE token_hash = ?1", params![hash])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Rate limiting (acceptance criterion 5: "max 5 failed attempts per IP per
// minute; 10s cooldown after 3 failures")
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum RateLimitDecision {
    Allow,
    Blocked { retry_after: Duration },
}

#[derive(Default)]
struct IpState {
    failures_in_window: u32,
    window_start: Option<DateTime<Utc>>,
    cooldown_until: Option<DateTime<Utc>>,
}

/// Per-IP login rate limiting. Takes `now` explicitly (fake-clock testable,
/// same convention as `lsp::Debouncer`/`mcp::ReconnectTracker`).
#[derive(Default)]
pub struct RateLimiter {
    state: HashMap<String, IpState>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `ip` may attempt a login right now.
    pub fn check(&mut self, ip: &str, now: DateTime<Utc>) -> RateLimitDecision {
        let entry = self.state.entry(ip.to_string()).or_default();
        if let Some(start) = entry.window_start
            && now - start > Duration::minutes(1)
        {
            *entry = IpState::default();
        }
        if let Some(until) = entry.cooldown_until
            && now < until
        {
            return RateLimitDecision::Blocked { retry_after: until - now };
        }
        if entry.failures_in_window >= 5 {
            let window_end = entry.window_start.unwrap_or(now) + Duration::minutes(1);
            if now < window_end {
                return RateLimitDecision::Blocked { retry_after: window_end - now };
            }
        }
        RateLimitDecision::Allow
    }

    /// Record a failed login attempt from `ip`: after 3 failures in the
    /// current 1-minute window, a 10s cooldown kicks in; the 5-per-minute
    /// cap is enforced by `check` reading `failures_in_window` directly.
    pub fn record_failure(&mut self, ip: &str, now: DateTime<Utc>) {
        let entry = self.state.entry(ip.to_string()).or_default();
        if entry.window_start.is_none() {
            entry.window_start = Some(now);
        }
        entry.failures_in_window += 1;
        if entry.failures_in_window == 3 {
            entry.cooldown_until = Some(now + Duration::seconds(10));
        }
    }

    /// A successful login clears any accumulated failure state for `ip`.
    pub fn record_success(&mut self, ip: &str) {
        self.state.remove(ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        ensure_webui_schema(&c).unwrap();
        c
    }

    // --- tokens --------------------------------------------------------

    #[test]
    fn generate_token_produces_a_64_char_hex_string() {
        let t = generate_token();
        assert_eq!(t.len(), 64, "32 bytes hex-encoded");
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_token_is_deterministic_and_never_equals_the_raw_token() {
        let t = generate_token();
        assert_eq!(hash_token(&t), hash_token(&t));
        assert_ne!(hash_token(&t), t);
    }

    #[test]
    fn issued_token_validates_to_the_right_username() {
        let c = conn();
        let now = "2026-01-01T00:00:00Z".parse().unwrap();
        let token = issue_token(&c, "rene", now).unwrap();
        assert_eq!(validate_token(&c, &token, now).unwrap(), Some("rene".to_string()));
    }

    #[test]
    fn an_unknown_token_does_not_validate() {
        let c = conn();
        let now = "2026-01-01T00:00:00Z".parse().unwrap();
        assert_eq!(validate_token(&c, "not-a-real-token", now).unwrap(), None);
    }

    #[test]
    fn an_expired_token_does_not_validate() {
        let c = conn();
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let token = issue_token(&c, "rene", now).unwrap();
        let later = now + TOKEN_TTL + Duration::seconds(1);
        assert_eq!(validate_token(&c, &token, later).unwrap(), None);
        // still valid one second before expiry
        let almost = now + TOKEN_TTL - Duration::seconds(1);
        assert_eq!(validate_token(&c, &token, almost).unwrap(), Some("rene".to_string()));
    }

    #[test]
    fn revoke_token_invalidates_it_immediately() {
        let c = conn();
        let now = "2026-01-01T00:00:00Z".parse().unwrap();
        let token = issue_token(&c, "rene", now).unwrap();
        revoke_token(&c, &token).unwrap();
        assert_eq!(validate_token(&c, &token, now).unwrap(), None);
    }

    #[test]
    fn refresh_pattern_revokes_the_old_token_and_issues_a_new_one() {
        let c = conn();
        let now = "2026-01-01T00:00:00Z".parse().unwrap();
        let old = issue_token(&c, "rene", now).unwrap();
        revoke_token(&c, &old).unwrap();
        let new = issue_token(&c, "rene", now).unwrap();
        assert_ne!(old, new);
        assert_eq!(validate_token(&c, &old, now).unwrap(), None);
        assert_eq!(validate_token(&c, &new, now).unwrap(), Some("rene".to_string()));
    }

    // --- rate limiting ---------------------------------------------------

    fn t(secs: i64) -> DateTime<Utc> {
        "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap() + Duration::seconds(secs)
    }

    #[test]
    fn allows_by_default() {
        let mut rl = RateLimiter::new();
        assert_eq!(rl.check("1.2.3.4", t(0)), RateLimitDecision::Allow);
    }

    #[test]
    fn three_failures_trigger_a_ten_second_cooldown() {
        let mut rl = RateLimiter::new();
        rl.record_failure("1.2.3.4", t(0));
        rl.record_failure("1.2.3.4", t(1));
        rl.record_failure("1.2.3.4", t(2));
        assert!(matches!(rl.check("1.2.3.4", t(2)), RateLimitDecision::Blocked { .. }));
        assert_eq!(rl.check("1.2.3.4", t(13)), RateLimitDecision::Allow, "cooldown elapsed after 10s");
    }

    #[test]
    fn five_failures_in_a_minute_block_until_the_window_resets() {
        let mut rl = RateLimiter::new();
        for i in 0..5 {
            rl.record_failure("1.2.3.4", t(i));
        }
        // past the 10s post-3rd-failure cooldown, but still within the 60s window
        assert!(matches!(rl.check("1.2.3.4", t(20)), RateLimitDecision::Blocked { .. }));
        assert_eq!(rl.check("1.2.3.4", t(61)), RateLimitDecision::Allow, "a full minute after the window started");
    }

    #[test]
    fn a_success_clears_accumulated_failures() {
        let mut rl = RateLimiter::new();
        rl.record_failure("1.2.3.4", t(0));
        rl.record_failure("1.2.3.4", t(1));
        rl.record_success("1.2.3.4");
        rl.record_failure("1.2.3.4", t(2));
        // only 1 failure since the reset -> not yet cooling down
        assert_eq!(rl.check("1.2.3.4", t(2)), RateLimitDecision::Allow);
    }

    #[test]
    fn different_ips_are_tracked_independently() {
        let mut rl = RateLimiter::new();
        for i in 0..5 {
            rl.record_failure("1.2.3.4", t(i));
        }
        assert!(matches!(rl.check("1.2.3.4", t(5)), RateLimitDecision::Blocked { .. }));
        assert_eq!(rl.check("5.6.7.8", t(5)), RateLimitDecision::Allow);
    }
}
