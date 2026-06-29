//! Curator (FEAT-024): consolidation pipeline — turns raw episodes and
//! transcripts into durable, topic-organized knowledge.
//!
//! The LLM produces *curator proposals* with an action (Add / Update / Delete /
//! Noop), a target memory id for mutations, a topic slug, and optional tags.
//! Application of proposals (and decay, promotion, pruning) is deterministic
//! and unit-tested. Only [`curate_once`] performs a network call.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::identity_db;
use crate::llm::MimirClient;
use crate::types::{ChatMessage, CuratorAction, CuratorProposal, CuratorStats, Episode, Memory, SessionRow};

const CURATE_CATEGORIES: &str = "fact, preference, pattern, project, skill, person";

/// Parse the LLM's curator output into proposals. Tolerant: extracts the
/// outermost JSON array from the text (the model may wrap it in prose/fences).
pub fn parse_curator_proposals(text: &str) -> Result<Vec<CuratorProposal>> {
    let slice = extract_json_array(text).context("no JSON array in curator output")?;
    let proposals: Vec<CuratorProposal> = serde_json::from_str(slice)
        .context("curator output is not a JSON array of CuratorProposal")?;
    Ok(proposals)
}

fn extract_json_array(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    (end >= start).then(|| &text[start..=end])
}

/// Read the inputs for a curation pass (unreflected episodes + current
/// memories + un-consolidated transcripts). Separated so the daemon can
/// release its DB lock before the network call.
pub fn gather_curation_inputs(
    conn: &Connection,
    episode_window: usize,
    transcript_window: usize,
) -> Result<(Vec<Episode>, Vec<Memory>, Vec<SessionRow>)> {
    let episodes = identity_db::episode_recent(conn, episode_window)?;
    let existing = identity_db::memory_list(conn, None)?;
    let sessions = identity_db::transcript_unconsolidated(conn, transcript_window)?;
    Ok((episodes, existing, sessions))
}

/// Build the curation prompt from recent episodes, existing memories, and
/// un-consolidated sessions.
pub fn curation_prompt(
    episodes: &[Episode],
    existing: &[Memory],
    sessions: &[SessionRow],
) -> String {
    let mut s = String::new();
    s.push_str(
        "You are the Curator: an agent's long-term memory consolidation system. \
         Review the recent activity, transcripts, and existing memories below.\n\n\
         For each useful piece of knowledge, propose an action:\n\
         - **Add**: a new fact/preference/pattern/project/skill/person memory.\n\
         - **Update**: modify an existing memory's content (provide its `target_id`).\n\
         - **Delete**: remove a stale or wrong memory (provide its `target_id`).\n\
         - **Noop**: explicitly skip something not worth keeping.\n\n\
         Follow these rules:\n\
         - Strongly prefer **Update** over creating a near-duplicate.\n\
         - Assign a `topic` slug (short kebab-case, e.g. \"disk-management\").\n\
         - `tags` are optional short labels for classification.\n\
         - Allowed categories: fact, preference, pattern, project, skill, person.\n\
         - Topic slug must be non-empty if provided; use existing slugs when possible.\n\n",
    );
    if !episodes.is_empty() {
        s.push_str("## Recent activity (episodes)\n");
        for e in episodes {
            s.push_str(&format!("- [{}] {}", e.kind, e.summary));
            if let Some(d) = &e.detail {
                s.push_str(&format!(" — {d}"));
            }
            s.push('\n');
        }
        s.push('\n');
    }
    if !sessions.is_empty() {
        s.push_str("## Un-consolidated sessions\n");
        for se in sessions {
            s.push_str(&format!(
                "- [{}] title=\"{}\" messages={} preview=\"{}\"\n",
                se.kind,
                se.title,
                se.message_count,
                se.transcript_preview.as_deref().unwrap_or(""),
            ));
        }
        s.push('\n');
    }
    if !existing.is_empty() {
        s.push_str("## Existing memories\n");
        for m in existing {
            let tier = if m.strength >= 5 { "long" } else { "medium" };
            s.push_str(&format!("- [{}][{}] {} (strength={})\n", m.category, tier, m.content, m.strength));
        }
        s.push('\n');
    }
    s.push_str(&format!(
        "Output ONLY a JSON array (no prose, no code fences) of objects with fields:\n\
         - \"action\": \"Add\" | \"Update\" | \"Delete\" | \"Noop\"\n\
         - \"category\": string (one of: {CURATE_CATEGORIES})\n\
         - \"content\": string\n\
         - \"target_id\": string or null (required for Update/Delete)\n\
         - \"topic\": string or null (kebab-case slug)\n\
         - \"tags\": array of strings\n\n\
         Output an empty array [] if nothing is worth doing.\n"
    ));
    s
}

/// Apply a set of curator proposals to the store. Deterministic (no LLM).
pub fn apply_curation(
    conn: &Connection,
    proposals: &[CuratorProposal],
) -> Result<CuratorStats> {
    let mut stats = CuratorStats::default();
    for p in proposals {
        let category = p.category.trim();
        let content = p.content.trim();
        if category.is_empty() || content.is_empty() {
            continue;
        }
        let modified = identity_db::curator_apply_proposal(conn, p)?;
        match p.action {
            CuratorAction::Add => { if modified { stats.added += 1; } }
            CuratorAction::Update => { if modified { stats.updated += 1; } }
            CuratorAction::Delete => { if modified { stats.deleted += 1; } }
            CuratorAction::Noop => { stats.noop += 1; }
        }
    }
    Ok(stats)
}

/// Mark episodes as consolidated and update session summaries from curator
/// output. This is called after proposals are applied.
pub fn mark_consolidated(
    conn: &Connection,
    episodes: &[Episode],
    sessions: &[SessionRow],
    proposal_topics: &[String],
) -> Result<CuratorStats> {
    let mut stats = CuratorStats::default();

    // Mark episodes consolidated.
    if !episodes.is_empty() {
        let ids: Vec<String> = episodes.iter().map(|e| e.id.clone()).collect();
        identity_db::episode_mark_reflected(conn, &ids)?;
        stats.episodes = episodes.len();
    }

    // Mark sessions as having a summary (set a placeholder so they aren't
    // re-processed). The actual summary is set by the prompt, but we mark
    // them here to prevent double-count.
    for se in sessions {
        let _ = conn.execute(
            "UPDATE sessions SET summary = COALESCE(summary, 'curated') WHERE id = ?1 AND (summary IS NULL OR summary = '')",
            rusqlite::params![&se.id],
        );
        stats.sessions += 1;
    }

    // Track topic count.
    stats.topics = proposal_topics.len();

    Ok(stats)
}

/// Run decay, promotion, and pruning after a curation pass.
pub fn post_curation_maintenance(
    conn: &Connection,
    decay_before: &str,
    promote_threshold: i64,
    prune_before: &str,
) -> Result<CuratorStats> {
    let mut stats = CuratorStats::default();

    // Promote medium→long past threshold.
    stats.promoted = identity_db::memory_promote(conn, promote_threshold)?;

    // Decay (medium faster, long more lenient).
    stats.decayed = identity_db::memory_decay(conn, decay_before)?;

    // Prune old consolidated episodes.
    stats.pruned = identity_db::episode_prune(conn, prune_before)?;

    Ok(stats)
}

/// Run one full curation pass: gather inputs, call LLM, apply proposals,
/// mark consolidated, then maintain (decay/promote/prune).
pub async fn curate_once(
    conn: &Connection,
    client: &MimirClient,
    episode_window: usize,
    transcript_window: usize,
    decay_before: &str,
    promote_threshold: i64,
    prune_before: &str,
) -> Result<CuratorStats> {
    let (episodes, existing, sessions) = gather_curation_inputs(conn, episode_window, transcript_window)?;

    if episodes.is_empty() && sessions.is_empty() {
        // Nothing to curate — still run maintenance.
        return post_curation_maintenance(conn, decay_before, promote_threshold, prune_before);
    }

    let prompt = curation_prompt(&episodes, &existing, &sessions);
    let text = client.chat_completion(&[ChatMessage::user(prompt)]).await?;

    // Parse errors leave episodes un-consolidated for the next pass.
    let proposals = parse_curator_proposals(&text)?;

    // Collect topic slugs from proposals.
    let topics: Vec<String> = proposals.iter()
        .filter_map(|p| p.topic.as_ref().filter(|t| !t.is_empty()).cloned())
        .collect();

    // Apply proposals.
    let mut stats = apply_curation(conn, &proposals)?;

    // Mark consolidated.
    let mark = mark_consolidated(conn, &episodes, &sessions, &topics)?;
    stats.episodes = mark.episodes;
    stats.sessions = mark.sessions;
    stats.topics = mark.topics;

    // Maintenance.
    let maint = post_curation_maintenance(conn, decay_before, promote_threshold, prune_before)?;
    stats.promoted = maint.promoted;
    stats.decayed = maint.decayed;
    stats.pruned = maint.pruned;

    Ok(stats)
}

/// Legacy reflection (FEAT-005/006): simpler reinforce-or-create, no
/// interference resolution. Used for backward compatibility. Delegates to
/// the curator pipeline with a simple prompt format.
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
        match identity_db::memory_find_similar(conn, category, content)? {
            Some(id) => {
                identity_db::memory_reinforce(conn, &id)?;
                stats.reinforced += 1;
            }
            None => {
                identity_db::memory_save_reflection(conn, category, content)?;
                stats.created += 1;
            }
        }
    }
    Ok(stats)
}

/// Parse the LLM's reflection output into proposals.
pub fn parse_proposals(text: &str) -> Result<Vec<ReflectionProposal>> {
    let slice = extract_json_array(text).context("no JSON array in reflection output")?;
    let proposals: Vec<ReflectionProposal> = serde_json::from_str(slice)
        .context("reflection output is not a JSON array of {category, content}")?;
    Ok(proposals)
}

const REFLECT_CATEGORIES: &str = "fact, preference, pattern, project, skill, person";

/// Read the inputs for a reflection pass (unreflected episodes + current
/// memories). Separated so the daemon can release its DB lock before the
/// network call.
pub fn gather(conn: &Connection, window: usize) -> Result<(Vec<Episode>, Vec<Memory>)> {
    let episodes = identity_db::episode_recent(conn, window)?;
    let existing = identity_db::memory_list(conn, None)?;
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
    identity_db::episode_mark_reflected(conn, &ids)?;
    stats.episodes = episodes.len();
    stats.decayed = identity_db::memory_decay(conn, decay_before)?;
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
/// reflection memories.
pub async fn reflect_once(
    conn: &Connection,
    client: &MimirClient,
    window: usize,
    decay_before: &str,
) -> Result<ReflectionStats> {
    let (episodes, existing) = gather(conn, window)?;
    if episodes.is_empty() {
        let decayed = identity_db::memory_decay(conn, decay_before)?;
        return Ok(ReflectionStats { decayed, ..Default::default() });
    }
    let prompt = reflection_prompt(&episodes, &existing);
    let text = client.chat_completion(&[ChatMessage::user(prompt)]).await?;
    let proposals = parse_proposals(&text)?;
    apply(conn, &episodes, &proposals, decay_before)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        identity_db::init_identity_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn parse_curator_proposals_extracts_json_from_prose() {
        let text = "Here are the actions:\n\
            [{\"action\":\"Add\",\"category\":\"fact\",\"content\":\"db01 is postgres\",\"topic\":\"database\",\"tags\":[\"db\"]},\
             {\"action\":\"Noop\",\"category\":\"pattern\",\"content\":\"old pattern\",\"target_id\":null,\"topic\":null,\"tags\":[]}]\nDone.";
        let p = parse_curator_proposals(text).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].action, CuratorAction::Add);
        assert_eq!(p[0].topic.as_deref(), Some("database"));
        assert_eq!(p[1].action, CuratorAction::Noop);
    }

    #[test]
    fn parse_curator_proposals_handles_empty_array() {
        assert!(parse_curator_proposals("nothing useful []").unwrap().is_empty());
    }

    #[test]
    fn parse_curator_proposals_errors_without_array() {
        assert!(parse_curator_proposals("no json here").is_err());
    }

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

    #[test]
    fn apply_curation_add_update_delete_noop() {
        let conn = test_conn();
        // Start with an existing memory.
        let m = identity_db::memory_save(&conn, "fact", "existing fact").unwrap();

        let proposals = vec![
            CuratorProposal {
                action: CuratorAction::Add,
                category: "fact".into(),
                content: "new fact".into(),
                target_id: None,
                topic: Some("general".into()),
                tags: vec![],
            },
            CuratorProposal {
                action: CuratorAction::Update,
                category: "fact".into(),
                content: "updated existing".into(),
                target_id: Some(m.id.clone()),
                topic: None,
                tags: vec![],
            },
            CuratorProposal {
                action: CuratorAction::Noop,
                category: "pattern".into(),
                content: "skip this".into(),
                target_id: None,
                topic: None,
                tags: vec![],
            },
        ];

        let stats = apply_curation(&conn, &proposals).unwrap();
        assert_eq!(stats.added, 1);
        assert_eq!(stats.updated, 1);
        assert_eq!(stats.noop, 1);
        assert_eq!(stats.deleted, 0);

        let mems = identity_db::memory_list(&conn, None).unwrap();
        assert_eq!(mems.len(), 2);
        assert!(mems.iter().any(|m| m.content == "new fact"));
        assert!(mems.iter().any(|m| m.content == "updated existing"));

        // Now delete the original.
        let del = vec![CuratorProposal {
            action: CuratorAction::Delete,
            category: "fact".into(),
            content: "delete".into(),
            target_id: Some(m.id),
            topic: None,
            tags: vec![],
        }];
        let stats = apply_curation(&conn, &del).unwrap();
        assert_eq!(stats.deleted, 1);
        assert_eq!(identity_db::memory_list(&conn, None).unwrap().len(), 1);
    }

    #[test]
    fn apply_reflection_reinforces_or_creates() {
        let conn = test_conn();
        identity_db::memory_save_reflection(&conn, "pattern", "disk pressure on db01").unwrap();
        let proposals = vec![
            ReflectionProposal { category: "pattern".into(), content: "disk pressure on db01".into() },
            ReflectionProposal { category: "fact".into(), content: "db01 runs postgres 16".into() },
        ];
        let stats = apply_reflection(&conn, &proposals).unwrap();
        assert_eq!(stats.reinforced, 1);
        assert_eq!(stats.created, 1);
        let mems = identity_db::memory_list(&conn, None).unwrap();
        assert_eq!(mems.iter().find(|m| m.content == "disk pressure on db01").unwrap().strength, 2);
        assert!(mems.iter().any(|m| m.category == "fact" && m.content == "db01 runs postgres 16"));
    }

    #[test]
    fn curation_prompt_includes_all_sections() {
        let episodes = vec![
            Episode { id: "e1".into(), kind: "task_run".into(), ref_id: None, summary: "ran check".into(), detail: None, created_at: "now".into(), reflected: false },
        ];
        let memories = vec![
            Memory { id: "m1".into(), category: "fact".into(), content: "db01 is postgres".into(), created_at: "".into(), updated_at: "".into(), strength: 3, last_seen: None, source: "reflection".into() },
        ];
        let sessions = vec![
            SessionRow { id: "s1".into(), host: None, kind: "chat".into(), title: "debug".into(), message_count: 5, token_count: 100, state: "closed".into(), transcript_preview: Some("hello".into()), summary: None, started_at: "now".into(), ended_at: Some("later".into()) },
        ];
        let prompt = curation_prompt(&episodes, &memories, &sessions);
        assert!(prompt.contains("task_run"));
        assert!(prompt.contains("db01 is postgres"));
        assert!(prompt.contains("debug"));
        assert!(prompt.contains("Add"));
        assert!(prompt.contains("Update"));
        assert!(prompt.contains("Delete"));
    }

    #[test]
    fn post_curation_maintenance_runs_all_steps() {
        let conn = test_conn();
        // Create medium memory and reinforce to promote.
        let m = identity_db::memory_save_reflection(&conn, "fact", "promotable").unwrap();
        for _ in 0..5 { identity_db::memory_reinforce(&conn, &m.id).unwrap(); }

        // Create medium memory that will decay.
        let _d = identity_db::memory_save_reflection(&conn, "fact", "decayable").unwrap();

        // Create an episode to prune.
        let ep = identity_db::episode_record(&conn, "task_run", None, "old", None).unwrap();
        identity_db::episode_mark_reflected(&conn, &[ep.id]).unwrap();

        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let stats = post_curation_maintenance(&conn, &future, 5, &future).unwrap();

        assert_eq!(stats.promoted, 1);
        assert_eq!(stats.decayed, 1);
        assert_eq!(stats.pruned, 1);
    }
}
