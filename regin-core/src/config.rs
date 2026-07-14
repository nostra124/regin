use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::debug;

/// System-wide skills directory (shipped with the package).
pub const SYSTEM_SKILLS_DIR: &str = "/usr/share/regin/skills";

/// System-wide desired-state (to-be) directory, shipped with the package (FEAT-033).
pub const SYSTEM_DESIRED_DIR: &str = "/usr/share/regin/desired";

/// System-wide notice-filter directory, shipped with the package (FEAT-052).
pub const SYSTEM_FILTERS_DIR: &str = "/usr/share/regin/filters";

/// System-wide operator-skill directory, shipped with the package (FEAT-045/046).
pub const SYSTEM_OPERATOR_SKILLS_DIR: &str = "/usr/share/regin/operator-skills";

/// System-wide plugin directory, shipped with the package (FEAT-082).
pub const SYSTEM_PLUGINS_DIR: &str = "/usr/share/regin/plugins";

/// Well-known settings keys and their defaults.
pub const SETTINGS: &[(&str, &str, &str)] = &[
    ("mimir.base_url", "http://127.0.0.1:8700/v1", "Mimir gateway OpenAI-compatible base URL"),
    ("mimir.fingerprint", "", "Mimir access credential — the approved client-cert SHA-256 fingerprint, sent as X-Client-Cert-Sha256 (provision via Dvalin / the Mimir console)"),
    ("mimir.model", "auto", "LLM model (auto routes via Mimir)"),
    ("daemon.enabled", "false", "Keep daemon running permanently via user systemd"),
    ("daemon.auto_register", "true", "On first use, auto-register the systemd user service (set false to only spawn transiently)"),
    ("memory.episodic_retention_days", "30", "Days to retain reflected episodic memories"),
    ("memory.reflect_interval", "daily", "How often the daemon reflects episodes into semantic memory"),
    ("memory.embeddings.enabled", "true", "Enable semantic retrieval via embeddings (FEAT-026); when disabled, falls back to FTS-only"),
    ("memory.embeddings.model", "auto", "Embedding model (auto routes via Mimir)"),
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
    ("decision.default_mode", "act", "Default decision mode when no Persona override applies: act|deliberate (FEAT-028)"),
    ("decision.deliberate.max_rounds", "3", "Plan/re-plan rounds before deliberate mode default-denies + escalates (FEAT-028)"),
    ("decision.deliberate.confidence_threshold", "0.7", "Minimum Soul-vote confidence for an approve to pass the gate; below this an approve is treated as revise (FEAT-029)"),
    ("decision.principles.recurrence_threshold", "3", "Minimum recurring bad-outcome deliberations before reflection proposes a candidate principle (FEAT-031)"),
    ("lsp.enabled", "false", "Feed LSP diagnostics back after write_file/edit_file/apply_patch — opt-in (FEAT-078)"),
    ("lsp.debounce_ms", "500", "Minimum interval between automatic diagnostics runs for the same file (FEAT-078)"),
    ("lsp.idle_timeout_secs", "300", "Idle time before a spawned language server process is recycled (FEAT-078)"),
    ("task.max_concurrency", "3", "Maximum subagents running concurrently via the `task` tool (FEAT-079)"),
    ("permission.bash", "allow", "Permission level for bash when no permission.bash.patterns rule matches: allow|ask|deny (FEAT-080)"),
    ("permission.bash.patterns", "[]", "JSON array of {pattern, level} glob rules for bash commands, last match wins (FEAT-080)"),
    ("permission.read_file", "allow", "Permission level for read_file: allow|ask|deny (FEAT-080)"),
    ("permission.write_file", "allow", "Permission level for write_file: allow|ask|deny (FEAT-080)"),
    ("permission.edit_file", "allow", "Permission level for edit_file: allow|ask|deny (FEAT-080)"),
    ("permission.apply_patch", "allow", "Permission level for apply_patch: allow|ask|deny (FEAT-080)"),
    ("permission.undo", "allow", "Permission level for undo: allow|ask|deny (FEAT-080)"),
    ("permission.undo_list", "allow", "Permission level for undo_list: allow|ask|deny (FEAT-080)"),
    ("permission.glob", "allow", "Permission level for glob: allow|ask|deny (FEAT-080)"),
    ("permission.grep", "allow", "Permission level for grep: allow|ask|deny (FEAT-080)"),
    ("permission.web_search", "allow", "Permission level for web_search: allow|ask|deny (FEAT-080)"),
    ("permission.diagnostics", "allow", "Permission level for diagnostics: allow|ask|deny (FEAT-080)"),
    ("permission.task", "allow", "Permission level for task (subagent delegation): allow|ask|deny (FEAT-080)"),
    ("permission.mcp.patterns", "[]", "JSON array of {pattern, level} glob rules for mcp_<server>_<tool> names, last match wins (FEAT-081)"),
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

/// Returns the identity DB path: ~/.local/share/regin/identity.db
pub fn identity_db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("identity.db"))
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

/// Returns user operator-skill dir: ~/.config/regin/operator-skills/ (FEAT-045)
pub fn user_operator_skills_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("regin").join("operator-skills"))
}

/// Returns system operator-skill dir: /usr/share/regin/operator-skills/ (FEAT-045)
pub fn system_operator_skills_dir() -> PathBuf {
    PathBuf::from(SYSTEM_OPERATOR_SKILLS_DIR)
}

/// Returns user plugin dir: ~/.config/regin/plugins/ (FEAT-082)
pub fn user_plugins_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("regin").join("plugins"))
}

/// Returns system plugin dir: /usr/share/regin/plugins/ (FEAT-082)
pub fn system_plugins_dir() -> PathBuf {
    PathBuf::from(SYSTEM_PLUGINS_DIR)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_and_derived_paths_are_scoped_under_regin() {
        assert!(data_dir().unwrap().ends_with("regin"));
        assert!(db_path().unwrap().ends_with("regin/regin.db"));
        assert!(identity_db_path().unwrap().ends_with("regin/identity.db"));
    }

    #[test]
    fn socket_path_prefers_xdg_runtime_dir_and_falls_back_without_it() {
        // Contained to a single test function, holding a whole-process env
        // lock, so this never races another concurrently-running test that
        // reads XDG_RUNTIME_DIR or XDG_CONFIG_HOME (see `xdg_env_lock`'s doc
        // comment in lib.rs).
        let _guard = crate::xdg_env_lock::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("XDG_RUNTIME_DIR").ok();

        unsafe { std::env::set_var("XDG_RUNTIME_DIR", "/tmp/feat075-runtime") };
        let with_runtime_dir = socket_path().unwrap();
        assert_eq!(with_runtime_dir, PathBuf::from("/tmp/feat075-runtime/regin/regind.sock"));

        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
        let fallback = socket_path().unwrap();
        assert!(fallback.ends_with("regin/regind.sock"));
        assert_ne!(fallback, with_runtime_dir, "fallback differs from the XDG_RUNTIME_DIR path");

        match saved {
            Some(v) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", v) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }

    #[test]
    fn user_and_system_dirs_are_scoped_under_regin() {
        let _guard = crate::xdg_env_lock::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(user_skills_dir().unwrap().ends_with("regin/skills"));
        assert_eq!(system_skills_dir(), PathBuf::from(SYSTEM_SKILLS_DIR));
        assert!(user_desired_dir().unwrap().ends_with("regin/desired"));
        assert_eq!(system_desired_dir(), PathBuf::from(SYSTEM_DESIRED_DIR));
        assert!(user_filters_dir().unwrap().ends_with("regin/filters"));
        assert_eq!(system_filters_dir(), PathBuf::from(SYSTEM_FILTERS_DIR));
        assert!(user_operator_skills_dir().unwrap().ends_with("regin/operator-skills"));
        assert_eq!(system_operator_skills_dir(), PathBuf::from(SYSTEM_OPERATOR_SKILLS_DIR));
        assert!(user_plugins_dir().unwrap().ends_with("regin/plugins"));
        assert_eq!(system_plugins_dir(), PathBuf::from(SYSTEM_PLUGINS_DIR));
    }

    #[test]
    fn systemd_paths_are_scoped_under_systemd_user() {
        let _guard = crate::xdg_env_lock::LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(user_systemd_dir().unwrap().ends_with("systemd/user"));
        assert!(regind_service_path().unwrap().ends_with("systemd/user/regind.service"));
    }

    #[test]
    fn regind_service_unit_embeds_the_binary_path_and_key_directives() {
        let unit = regind_service_unit("/usr/bin/regind");
        assert!(unit.contains("ExecStart=/usr/bin/regind"));
        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("WantedBy=default.target"));
    }
}
