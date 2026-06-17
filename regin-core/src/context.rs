//! Build system context from layered context files and memories.
//!
//! Context loading order:
//! 1. ~/.config/regin/context.md (global user context)
//! 2. <cwd>/.repo/regin/context.md (repo-local context)
//! 3. All memories from the database

use crate::types::Memory;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Paths to check for context files.
fn context_paths(cwd: Option<&str>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. Global user context
    if let Some(config) = dirs::config_dir() {
        paths.push(config.join("regin").join("context.md"));
    }

    // 2. Repo-local context
    if let Some(dir) = cwd {
        paths.push(Path::new(dir).join(".repo").join("regin").join("context.md"));
    }

    paths
}

/// Read context files (silently skip missing/unreadable).
fn load_context_files(cwd: Option<&str>) -> Vec<(String, String)> {
    let mut parts = Vec::new();
    for path in context_paths(cwd) {
        match std::fs::read_to_string(&path) {
            Ok(content) if !content.trim().is_empty() => {
                debug!("Loaded context: {}", path.display());
                let label = if path.to_string_lossy().contains(".repo") {
                    "repo context"
                } else {
                    "user context"
                };
                parts.push((label.to_string(), content));
            }
            _ => {}
        }
    }
    parts
}

/// Build a system prompt from context files + memories.
pub fn build_system_prompt(cwd: Option<&str>, memories: &[Memory]) -> Option<String> {
    let files = load_context_files(cwd);
    if files.is_empty() && memories.is_empty() {
        return None;
    }

    let mut parts = Vec::new();

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
