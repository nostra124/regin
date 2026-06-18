//! Hermes reflection (FEAT-006): distil the episodic tier into durable semantic
//! memories, reinforcing recurring signals and decaying stale ones.
//!
//! The LLM produces *proposals*; the application of those proposals
//! (reinforce-or-create) and the decay pass are deterministic and unit-tested.
//! Only [`reflect_once`] performs a network call.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::llm::NanoGptClient;
use crate::types::{ChatMessage, Episode, Memory};

/// A proposed semantic memory produced by reflection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionProposal {
    pub category: String,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReflectionStats {
    pub episodes: usize,
    pub reinforced: usize,
    pub created: usize,
    pub decayed: usize,
}

/// Apply proposals to the semantic store: reinforce a matching memory, else
/// create a new reflection memory. Deterministic (no LLM).
pub fn apply_reflection(conn: &Connection, proposals: &[ReflectionProposal]) -> Result<ReflectionStats> {
    let mut stats = ReflectionStats::default();
    for p in proposals {
        let category = p.category.trim();
        let content = p.content.trim();
        if category.is_empty() || content.is_empty() {
            continue;
        }
        match db::memory_find_similar(conn, category, content)? {
            Some(id) => {
                db::memory_reinforce(conn, &id)?;
                stats.reinforced += 1;
            }
            None => {
                db::memory_save_reflection(conn, category, content)?;
                stats.created += 1;
            }
        }
    }
    Ok(stats)
}

/// Parse the LLM's reflection output into proposals. Tolerant: extracts the
/// outermost JSON array from the text (the model may wrap it in prose/fences).
pub fn parse_proposals(text: &str) -> Result<Vec<ReflectionProposal>> {
    let slice = extract_json_array(text).context("no JSON array in reflection output")?;
    let proposals: Vec<ReflectionProposal> = serde_json::from_str(slice)
        .context("reflection output is not a JSON array of {category, content}")?;
    Ok(proposals)
}

fn extract_json_array(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    (end >= start).then(|| &text[start..=end])
}

const REFLECT_CATEGORIES: &str = "fact, preference, pattern, project, skill, person";

/// Read the inputs for a reflection pass (unreflected episodes + current
/// memories). Separated so the daemon can release its DB lock before the
/// network call.
pub fn gather(conn: &Connection, window: usize) -> Result<(Vec<Episode>, Vec<Memory>)> {
    let episodes = db::episode_recent(conn, window)?;
    let existing = db::memory_list(conn, None)?;
    Ok((episodes, existing))
}

/// Apply parsed proposals, mark the episodes reflected, and decay stale
/// reflection memories. Deterministic (no LLM).
pub fn apply(
    conn: &Connection,
    episodes: &[Episode],
    proposals: &[ReflectionProposal],
    decay_before: &str,
) -> Result<ReflectionStats> {
    let mut stats = apply_reflection(conn, proposals)?;
    let ids: Vec<String> = episodes.iter().map(|e| e.id.clone()).collect();
    db::episode_mark_reflected(conn, &ids)?;
    stats.episodes = episodes.len();
    stats.decayed = db::memory_decay(conn, decay_before)?;
    Ok(stats)
}

/// Build the reflection prompt from recent episodes + existing memories.
pub fn reflection_prompt(episodes: &[Episode], existing: &[Memory]) -> String {
    let mut s = String::new();
    s.push_str(
        "You maintain an operations agent's long-term memory. Review the recent activity below \
         and distil any durable, reusable knowledge into memories. Strongly prefer reinforcing an \
         existing memory (reuse its EXACT category and content text) over creating a near-duplicate.\n\n",
    );
    s.push_str("## Recent activity (episodes)\n");
    for e in episodes {
        s.push_str(&format!("- [{}] {}", e.kind, e.summary));
        if let Some(d) = &e.detail {
            s.push_str(&format!(" \u{2014} {d}"));
        }
        s.push('\n');
    }
    s.push_str("\n## Existing memories\n");
    for m in existing {
        s.push_str(&format!("- [{}] {}\n", m.category, m.content));
    }
    s.push_str(&format!(
        "\nOutput ONLY a JSON array (no prose, no code fences) of objects \
         {{\"category\": \"...\", \"content\": \"...\"}}. Allowed categories: {REFLECT_CATEGORIES}. \
         Output an empty array [] if nothing is worth remembering.\n"
    ));
    s
}

/// Run one reflection pass: pull unreflected episodes, ask the LLM to distil
/// them, apply the proposals, mark the episodes reflected, then decay stale
/// reflection memories. `window` bounds the episodes considered; `decay_before`
/// is the decay cutoff (RFC3339).
pub async fn reflect_once(
    conn: &Connection,
    client: &NanoGptClient,
    window: usize,
    decay_before: &str,
) -> Result<ReflectionStats> {
    let (episodes, existing) = gather(conn, window)?;
    if episodes.is_empty() {
        let decayed = db::memory_decay(conn, decay_before)?;
        return Ok(ReflectionStats { decayed, ..Default::default() });
    }
    let prompt = reflection_prompt(&episodes, &existing);
    let text = client.chat_completion(&[ChatMessage::user(prompt)]).await?;
    // Parse errors leave the episodes unreflected for the next pass.
    let proposals = parse_proposals(&text)?;
    apply(conn, &episodes, &proposals, decay_before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proposals_extracts_json_from_prose() {
        let text = "Here are the memories:\n\
            [{\"category\":\"fact\",\"content\":\"db01 is postgres\"},\
             {\"category\":\"pattern\",\"content\":\"logs fill weekly\"}]\nDone.";
        let p = parse_proposals(text).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].category, "fact");
        assert_eq!(p[1].content, "logs fill weekly");
    }

    #[test]
    fn parse_proposals_handles_empty_array() {
        assert!(parse_proposals("nothing useful []").unwrap().is_empty());
    }

    #[test]
    fn parse_proposals_errors_without_array() {
        assert!(parse_proposals("no json here").is_err());
    }
}
