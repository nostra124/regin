//! Build system context from the global user context file, the per-repo context
//! (stored in regin's own DB, keyed by repo path — FEAT-008), and memories.

use crate::types::Memory;
use tracing::debug;

/// The global user context file (`~/.config/regin/context.md`), if non-empty.
fn global_user_context() -> Option<String> {
    let path = dirs::config_dir()?.join("regin").join("context.md");
    match std::fs::read_to_string(&path) {
        Ok(c) if !c.trim().is_empty() => {
            debug!("Loaded global user context: {}", path.display());
            Some(c)
        }
        _ => None,
    }
}

/// The "senseful full automation" operator directive (FEAT-049 / DISC-015): the
/// baseline of regin's operator-plane system prompt.
pub const OPERATOR_DIRECTIVE: &str = "\
You are regin, an autonomous operator of this Linux machine. Keep it at its \
to-be state with *senseful* full automation: act directly on safe, reversible \
fixes; stage risky or destructive changes for human approval; escalate what is \
beyond your control. Minimise cost while holding reliability — prefer cheap, \
deterministic checks over repeated LLM judgement, and never cross the global \
red-lines (the safety substrate, your own governance, or catastrophic host \
actions).";

/// Build a system prompt: the operator directive first, then the global user
/// context, the per-repo context (already resolved by the caller), and memories.
pub fn build_system_prompt(repo_context: Option<&str>, memories: &[Memory]) -> Option<String> {
    let mut files: Vec<(&str, String)> = Vec::new();
    if let Some(g) = global_user_context() {
        files.push(("user context", g));
    }
    if let Some(rc) = repo_context
        && !rc.trim().is_empty()
    {
        files.push(("repo context", rc.to_string()));
    }

    let mut parts = vec![OPERATOR_DIRECTIVE.to_string()];

    for (label, content) in &files {
        parts.push(format!("## {label}\n\n{content}"));
    }

    if !memories.is_empty() {
        parts.push("## Memories\n".to_string());
        for mem in memories {
            parts.push(format!("[{}] {}", mem.category, mem.content));
        }
        parts.push(String::new());
        parts.push(
            "You have a memory system. When you learn something important, \
             want to remember a preference, notice a pattern, or develop a useful skill, \
             tell the user you'd like to save it. The user can then run:\n\
             \n\
             regin memory save <category> \"<content>\"\n\
             \n\
             Categories: preference, fact, skill, pattern, project, person"
                .to_string(),
        );
    }

    Some(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_memory(category: &str, content: &str) -> Memory {
        Memory {
            id: "m1".into(),
            category: category.into(),
            content: content.into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            strength: 1,
            last_seen: None,
            source: "human".into(),
        }
    }

    /// Every test here transitively calls `global_user_context()`, which
    /// reads `XDG_CONFIG_HOME` — hold the whole-crate env lock throughout
    /// (see `xdg_env_lock`'s doc comment in lib.rs) so a concurrently-running
    /// `config.rs` test never observes a stale override, and vice versa.
    fn locked() -> std::sync::MutexGuard<'static, ()> {
        crate::xdg_env_lock::LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn includes_the_operator_directive_with_no_repo_context_or_memories() {
        let _guard = locked();
        let prompt = build_system_prompt(None, &[]).unwrap();
        assert!(prompt.contains(OPERATOR_DIRECTIVE));
        assert!(!prompt.contains("## repo context"));
        assert!(!prompt.contains("## Memories"));
    }

    #[test]
    fn includes_a_non_empty_repo_context_section() {
        let _guard = locked();
        let prompt = build_system_prompt(Some("do X before Y"), &[]).unwrap();
        assert!(prompt.contains("## repo context"));
        assert!(prompt.contains("do X before Y"));
    }

    #[test]
    fn excludes_a_whitespace_only_repo_context() {
        let _guard = locked();
        let prompt = build_system_prompt(Some("   \n  "), &[]).unwrap();
        assert!(!prompt.contains("## repo context"));
    }

    #[test]
    fn includes_a_memories_section_with_category_and_content() {
        let _guard = locked();
        let mems = vec![a_memory("fact", "the sky is blue")];
        let prompt = build_system_prompt(None, &mems).unwrap();
        assert!(prompt.contains("## Memories"));
        assert!(prompt.contains("[fact] the sky is blue"));
        assert!(prompt.contains("regin memory save"));
    }

    #[test]
    fn includes_the_global_user_context_file_when_present_and_non_empty() {
        let _guard = locked();
        let dir = std::env::temp_dir().join(format!("feat075-context-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("regin")).unwrap();
        std::fs::write(dir.join("regin").join("context.md"), "always run tests before pushing").unwrap();

        let saved = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };

        let prompt = build_system_prompt(None, &[]).unwrap();

        match saved {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(prompt.contains("## user context"));
        assert!(prompt.contains("always run tests before pushing"));
    }

    #[test]
    fn global_user_context_is_none_when_the_file_is_whitespace_only() {
        let _guard = locked();
        let dir = std::env::temp_dir().join(format!("feat075-context-empty-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("regin")).unwrap();
        std::fs::write(dir.join("regin").join("context.md"), "   \n  ").unwrap();

        let saved = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };

        let prompt = build_system_prompt(None, &[]).unwrap();

        match saved {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!prompt.contains("## user context"));
    }
}
