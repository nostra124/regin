use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::debug;

/// System-wide skills directory (shipped with the .deb package).
pub const SYSTEM_SKILLS_DIR: &str = "/usr/share/regin/skills";

/// Well-known settings keys and their defaults.
pub const SETTINGS: &[(&str, &str, &str)] = &[
    ("nanogpt.api_key", "", "NanoGPT API key"),
    ("nanogpt.model", "claude-sonnet-4-20250514", "LLM model"),
    ("nanogpt.base_url", "https://nano-gpt.com/api/v1", "NanoGPT API base URL"),
    ("daemon.enabled", "false", "Keep daemon running permanently via user systemd"),
    ("daemon.auto_register", "true", "On first use, auto-register the systemd user service (set false to only spawn transiently)"),
    ("memory.episodic_retention_days", "30", "Days to retain reflected episodic memories"),
    ("monitor.auto_incident", "false", "Auto-open incidents from failed scheduled runs"),
    ("monitor.severity", "medium", "Severity for auto-opened monitor incidents"),
    ("monitor.recurrence_threshold", "3", "Incidents of one skill before a problem is opened"),
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
