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
