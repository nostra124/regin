//! FEAT-080 (DISC-021): granular tool permissions. Each tool has a
//! permission level — `allow` / `ask` / `deny` — resolved fresh from SQLite
//! settings on every call, the same "no cache, so nothing to invalidate"
//! convention as `lsp::resolve_command`/`AppState::llm_client`:
//! `regin config set permission.<tool> <level>` takes effect on the very
//! next tool call, no daemon restart, no invalidation logic to get wrong
//! (acceptance criterion 7's "cache invalidation" requirement is satisfied
//! by there being no cache).

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    Allow,
    Ask,
    Deny,
}

impl PermissionLevel {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }
}

/// One glob-pattern -> level rule for `bash` command matching (acceptance
/// criterion 2). Stored as a JSON *array* (`permission.bash.patterns`)
/// rather than a JSON object — a JSON array has an unambiguous element
/// order to drive "last match wins" from; a JSON object's key order isn't
/// something callers should have to rely on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BashPatternRule {
    pub pattern: String,
    pub level: PermissionLevel,
}

/// Resolve the effective permission level for `tool`. For `bash`, `command`
/// (the command string about to run) is matched against
/// `permission.bash.patterns` — a JSON array of [`BashPatternRule`], last
/// match wins (criterion 2). With no match (or no patterns configured), and
/// for every other tool, falls back to the flat `permission.<tool>` setting
/// (default `allow` — criterion 5).
pub fn resolve_permission(conn: &rusqlite::Connection, tool: &str, command: Option<&str>) -> Result<PermissionLevel> {
    if tool == "bash"
        && let Some(cmd) = command
        && let Some(level) = resolve_bash_pattern(conn, cmd)?
    {
        return Ok(level);
    }
    let key = format!("permission.{tool}");
    let raw = crate::db::setting_get(conn, &key)?;
    Ok(PermissionLevel::parse(&raw).unwrap_or(PermissionLevel::Allow))
}

fn resolve_bash_pattern(conn: &rusqlite::Connection, command: &str) -> Result<Option<PermissionLevel>> {
    let raw = crate::db::setting_get(conn, "permission.bash.patterns")?;
    if raw.trim().is_empty() || raw.trim() == "[]" {
        return Ok(None);
    }
    let rules: Vec<BashPatternRule> = serde_json::from_str(&raw).unwrap_or_default();
    let mut matched = None;
    for rule in &rules {
        if glob_matches(&rule.pattern, command) {
            matched = Some(rule.level);
        }
    }
    Ok(matched)
}

fn glob_matches(pattern: &str, text: &str) -> bool {
    globset::Glob::new(pattern).map(|g| g.compile_matcher().is_match(text)).unwrap_or(false)
}

/// The refusal message surfaced to the LLM for a `deny`'d tool (acceptance
/// criterion 3).
pub fn denied_message(tool: &str) -> String {
    format!("Tool {tool} is disabled by policy.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_schema(&c).unwrap();
        c
    }

    #[test]
    fn level_parses_and_round_trips() {
        assert_eq!(PermissionLevel::parse("allow"), Some(PermissionLevel::Allow));
        assert_eq!(PermissionLevel::parse("ask"), Some(PermissionLevel::Ask));
        assert_eq!(PermissionLevel::parse("deny"), Some(PermissionLevel::Deny));
        assert_eq!(PermissionLevel::parse("whenever"), None);
        assert_eq!(PermissionLevel::parse(PermissionLevel::Ask.as_str()), Some(PermissionLevel::Ask));
    }

    // --- criterion 5: default allow, backward compatible ------------------

    #[test]
    fn unconfigured_tools_default_to_allow() {
        let c = conn();
        assert_eq!(resolve_permission(&c, "read_file", None).unwrap(), PermissionLevel::Allow);
        assert_eq!(resolve_permission(&c, "bash", Some("echo hi")).unwrap(), PermissionLevel::Allow);
    }

    // --- criterion 1: per-tool flat level ----------------------------------

    #[test]
    fn a_configured_flat_level_is_used() {
        let c = conn();
        crate::db::setting_set(&c, "permission.write_file", "deny").unwrap();
        assert_eq!(resolve_permission(&c, "write_file", None).unwrap(), PermissionLevel::Deny);
        crate::db::setting_set(&c, "permission.edit_file", "ask").unwrap();
        assert_eq!(resolve_permission(&c, "edit_file", None).unwrap(), PermissionLevel::Ask);
    }

    #[test]
    fn denied_message_names_the_tool() {
        assert_eq!(denied_message("bash"), "Tool bash is disabled by policy.");
    }

    // --- criterion 2: bash glob patterns, last match wins ------------------

    #[test]
    fn bash_pattern_matching_is_literal_wildcard_and_prefix() {
        let c = conn();
        crate::db::setting_set(
            &c, "permission.bash.patterns",
            r#"[{"pattern":"git status","level":"allow"},{"pattern":"git push *","level":"ask"},{"pattern":"rm -rf *","level":"deny"}]"#,
        ).unwrap();

        assert_eq!(resolve_permission(&c, "bash", Some("git status")).unwrap(), PermissionLevel::Allow, "literal match");
        assert_eq!(resolve_permission(&c, "bash", Some("git push origin main")).unwrap(), PermissionLevel::Ask, "prefix + wildcard match");
        assert_eq!(resolve_permission(&c, "bash", Some("rm -rf /tmp/x")).unwrap(), PermissionLevel::Deny, "wildcard match");
        // no pattern matches -> falls back to the flat permission.bash setting (unset -> allow)
        assert_eq!(resolve_permission(&c, "bash", Some("ls -la")).unwrap(), PermissionLevel::Allow);
    }

    #[test]
    fn bash_pattern_last_match_wins() {
        let c = conn();
        crate::db::setting_set(
            &c, "permission.bash.patterns",
            r#"[{"pattern":"*","level":"allow"},{"pattern":"git push *","level":"ask"},{"pattern":"git push --force*","level":"deny"}]"#,
        ).unwrap();

        assert_eq!(resolve_permission(&c, "bash", Some("echo hi")).unwrap(), PermissionLevel::Allow, "only the wildcard-all rule matches");
        assert_eq!(resolve_permission(&c, "bash", Some("git push origin main")).unwrap(), PermissionLevel::Ask, "the more specific later rule wins over '*'");
        assert_eq!(resolve_permission(&c, "bash", Some("git push --force origin main")).unwrap(), PermissionLevel::Deny, "the last (most specific) matching rule wins");
    }

    #[test]
    fn bash_falls_back_to_the_flat_setting_when_no_pattern_matches_or_none_configured() {
        let c = conn();
        crate::db::setting_set(&c, "permission.bash", "ask").unwrap();
        assert_eq!(resolve_permission(&c, "bash", Some("anything")).unwrap(), PermissionLevel::Ask, "no patterns configured at all");

        crate::db::setting_set(&c, "permission.bash.patterns", r#"[{"pattern":"ls *","level":"allow"}]"#).unwrap();
        assert_eq!(resolve_permission(&c, "bash", Some("echo hi")).unwrap(), PermissionLevel::Ask, "no pattern matches -> flat setting");
        assert_eq!(resolve_permission(&c, "bash", Some("ls -la")).unwrap(), PermissionLevel::Allow, "pattern matches -> overrides the flat setting");
    }

    #[test]
    fn bash_with_no_command_string_ignores_patterns() {
        let c = conn();
        crate::db::setting_set(&c, "permission.bash.patterns", r#"[{"pattern":"*","level":"deny"}]"#).unwrap();
        assert_eq!(resolve_permission(&c, "bash", None).unwrap(), PermissionLevel::Allow, "no command to match against -> flat default");
    }

    // --- criterion 7: "cache invalidation" == there is no cache -----------

    #[test]
    fn permission_changes_take_effect_immediately_no_cache_to_invalidate() {
        let c = conn();
        assert_eq!(resolve_permission(&c, "bash", Some("echo hi")).unwrap(), PermissionLevel::Allow);
        crate::db::setting_set(&c, "permission.bash", "deny").unwrap();
        assert_eq!(resolve_permission(&c, "bash", Some("echo hi")).unwrap(), PermissionLevel::Deny, "the very next resolve sees the change");
    }
}
