use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::debug;

/// System-wide skills directory (shipped with the package).
pub const SYSTEM_SKILLS_DIR: &str = "/usr/share/regin/skills";

/// System-wide desired-state (to-be) directory, shipped with the package (FEAT-033).
pub const SYSTEM_DESIRED_DIR: &str = "/usr/share/regin/desired";

/// System-wide notice-filter directory, shipped with the package (FEAT-052).
pub const SYSTEM_FILTERS_DIR: &str = "/usr/share/regin/filters";

/// Well-known settings keys and their defaults.
pub const SETTINGS: &[(&str, &str, &str)] = &[
    ("nanogpt.api_key", "", "NanoGPT API key"),
    ("nanogpt.model", "auto", "LLM model (auto routes via the subscription)"),
    ("nanogpt.base_url", "https://nano-gpt.com/api/v1", "NanoGPT API base URL"),
    ("daemon.enabled", "false", "Keep daemon running permanently via user systemd"),
    ("daemon.auto_register", "true", "On first use, auto-register the systemd user service (set false to only spawn transiently)"),
    ("memory.episodic_retention_days", "30", "Days to retain reflected episodic memories"),
    ("memory.reflect_interval", "daily", "How often the daemon reflects episodes into semantic memory"),
    ("memory.decay_days", "30", "Reflection memories unseen this long lose strength (then drop at 0)"),
    ("monitor.auto_incident", "false", "Auto-open incidents from failed scheduled runs"),
    ("monitor.severity", "medium", "Severity for auto-opened monitor incidents"),
    ("monitor.recurrence_threshold", "3", "Incidents of one skill before a problem is opened"),
    ("kpi.reliability_floor", "0.95", "Minimum incident-resolution rate the CSI objective must hold (cost is minimized subject to this)"),
    ("bus.last_ok", "", "Last successful supervisor-bus interaction (RFC3339); drives effective-mode detection"),
    ("bus.failures", "0", "Consecutive supervisor-bus failures since the last success (effective-mode debounce)"),
    ("posture.allow_auto", "true", "Master switch bounding how much may auto-apply; false forces conservative (FEAT-040)"),
    ("posture.min_samples", "10", "Minimum change outcomes before auto-apply can graduate (FEAT-040)"),
    ("posture.min_success_rate", "0.9", "Change-success rate required to graduate auto-apply (FEAT-040)"),
    ("posture.max_promotion_error_rate", "0.1", "Promotion-error rate above which posture demotes to conservative (FEAT-040)"),
    ("push.enabled", "false", "Opt-in active push for critical items (off by default, FEAT-044)"),
    ("push.channel", "none", "Active-push channel: none|ntfy|webhook (FEAT-044)"),
    ("push.target", "", "Active-push target URL (ntfy topic URL or webhook endpoint)"),
    ("push.min_severity", "critical", "Minimum severity to actively push (FEAT-044)"),
    ("push.min_interval_secs", "300", "Minimum seconds between active pushes (rate limit)"),
    ("regind.heartbeat", "", "Last scheduler tick (RFC3339); a stale value signals a stalled loop (FEAT-048)"),
];

/// Returns the XDG data directory for regin: ~/.local/share/regin/
pub fn data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("Cannot determine XDG data directory")?;
    Ok(base.join("regin"))
}

/// Returns the DB path: ~/.local/share/regin/regin.db
pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("regin.db"))
}

/// Returns the socket path: $XDG_RUNTIME_DIR/regin/regind.sock
/// Falls back to ~/.local/state/regin/regind.sock
pub fn socket_path() -> Result<PathBuf> {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(runtime).join("regin").join("regind.sock");
        debug!("Socket path (runtime): {}", p.display());
        return Ok(p);
    }
    let base = dirs::state_dir()
        .or_else(dirs::data_dir)
        .context("Cannot determine state directory")?;
    let p = base.join("regin").join("regind.sock");
    debug!("Socket path (fallback): {}", p.display());
    Ok(p)
}

/// Returns user skills dir: ~/.config/regin/skills/
pub fn user_skills_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("regin").join("skills"))
}

/// Returns system skills dir: /usr/share/regin/skills/
pub fn system_skills_dir() -> PathBuf {
    PathBuf::from(SYSTEM_SKILLS_DIR)
}

/// Returns user desired-state dir: ~/.config/regin/desired/ (FEAT-033)
pub fn user_desired_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("regin").join("desired"))
}

/// Returns system desired-state dir: /usr/share/regin/desired/ (FEAT-033)
pub fn system_desired_dir() -> PathBuf {
    PathBuf::from(SYSTEM_DESIRED_DIR)
}

/// Returns user notice-filter dir: ~/.config/regin/filters/ (FEAT-052)
pub fn user_filters_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("regin").join("filters"))
}

/// Returns system notice-filter dir: /usr/share/regin/filters/ (FEAT-052)
pub fn system_filters_dir() -> PathBuf {
    PathBuf::from(SYSTEM_FILTERS_DIR)
}

/// Returns the user systemd unit dir: ~/.config/systemd/user/
pub fn user_systemd_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("systemd").join("user"))
}

/// Returns the path to the regind user service file.
pub fn regind_service_path() -> Result<PathBuf> {
    Ok(user_systemd_dir()?.join("regind.service"))
}

/// Generate the systemd user unit file content for regind.
pub fn regind_service_unit(regind_bin: &str) -> String {
    format!(
        r#"[Unit]
Description=Regin Monitoring Daemon
After=default.target

[Service]
Type=exec
ExecStart={regind_bin}
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
"#
    )
}
