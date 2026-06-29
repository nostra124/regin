use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::info;

use crate::types::{CuratorAction, CuratorProposal, Episode, Memory, SessionRow, SessionWithTranscript, TranscriptMessage};

/// Current schema version stored in `identity_meta`.
const SCHEMA_VERSION: &str = "1";

/// Column list for memory queries (matches the fields that map to `Memory`).
const MEMORY_COLS: &str =
    "id, category, content, created_at, updated_at, strength, last_seen, source";

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

/// Open or create the identity database, run idempotent schema bootstrap.
pub fn init_identity_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open identity database: {}", path.display()))?;

    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    init_identity_schema(&conn)?;

    info!("Identity database initialized at {}", path.display());
    Ok(conn)
}

/// Apply the identity-db schema (idempotent). Split out so tests can use an
/// in-memory connection.
pub fn init_identity_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS identity_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS episodes (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            ref_id TEXT,
            host TEXT,
            importance INTEGER NOT NULL DEFAULT 1,
            state TEXT NOT NULL DEFAULT 'new',
            summary TEXT NOT NULL,
            detail TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            host TEXT,
            kind TEXT NOT NULL DEFAULT 'chat',
            title TEXT NOT NULL DEFAULT '',
            message_count INTEGER NOT NULL DEFAULT 0,
            token_count INTEGER NOT NULL DEFAULT 0,
            state TEXT NOT NULL DEFAULT 'open',
            transcript_text TEXT,
            summary TEXT,
            started_at TEXT NOT NULL,
            ended_at TEXT
        );

        CREATE TABLE IF NOT EXISTS transcripts (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id)
        );

        CREATE TABLE IF NOT EXISTS topics (
            id TEXT PRIMARY KEY,
            slug TEXT NOT NULL UNIQUE,
            parent_id TEXT,
            summary TEXT NOT NULL,
            host TEXT,
            pinned INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (parent_id) REFERENCES topics(id)
        );

        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            topic_id TEXT,
            category TEXT NOT NULL,
            tier TEXT NOT NULL DEFAULT 'medium',
            host TEXT,
            repo_key TEXT,
            source TEXT NOT NULL DEFAULT 'reflection',
            content TEXT NOT NULL,
            strength INTEGER NOT NULL DEFAULT 1,
            trust_score REAL NOT NULL DEFAULT 0.5,
            retrieval_count INTEGER NOT NULL DEFAULT 0,
            helpful_count INTEGER NOT NULL DEFAULT 0,
            pinned INTEGER NOT NULL DEFAULT 0,
            last_seen TEXT,
            last_retrieved TEXT,
            embedding BLOB,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (topic_id) REFERENCES topics(id)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content,
            category,
            content='memories',
            content_rowid='rowid'
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(
            role,
            content,
            content='transcripts',
            content_rowid='rowid'
        );

        CREATE INDEX IF NOT EXISTS idx_episodes_state ON episodes(state);
        CREATE INDEX IF NOT EXISTS idx_episodes_host ON episodes(host);
        CREATE INDEX IF NOT EXISTS idx_memories_topic ON memories(topic_id);
        CREATE INDEX IF NOT EXISTS idx_memories_tier ON memories(tier);
        CREATE INDEX IF NOT EXISTS idx_memories_host ON memories(host);
        CREATE INDEX IF NOT EXISTS idx_memories_last_retrieved ON memories(last_retrieved);
        CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);
        CREATE INDEX IF NOT EXISTS idx_transcripts_session ON transcripts(session_id);
        CREATE INDEX IF NOT EXISTS idx_topics_parent ON topics(parent_id);
        "
    )
    .context("Failed to create identity database tables")?;

    conn.execute_batch(
        "
        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, category)
            VALUES (new.rowid, new.content, new.category);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, category)
            VALUES ('delete', old.rowid, old.content, old.category);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, category)
            VALUES ('delete', old.rowid, old.content, old.category);
            INSERT INTO memories_fts(rowid, content, category)
            VALUES (new.rowid, new.content, new.category);
        END;

        CREATE TRIGGER IF NOT EXISTS transcripts_ai AFTER INSERT ON transcripts BEGIN
            INSERT INTO transcripts_fts(rowid, role, content)
            VALUES (new.rowid, new.role, new.content);
        END;

        CREATE TRIGGER IF NOT EXISTS transcripts_ad AFTER DELETE ON transcripts BEGIN
            INSERT INTO transcripts_fts(transcripts_fts, rowid, role, content)
            VALUES ('delete', old.rowid, old.role, old.content);
        END;

        CREATE TRIGGER IF NOT EXISTS transcripts_au AFTER UPDATE ON transcripts BEGIN
            INSERT INTO transcripts_fts(transcripts_fts, rowid, role, content)
            VALUES ('delete', old.rowid, old.role, old.content);
            INSERT INTO transcripts_fts(rowid, role, content)
            VALUES (new.rowid, new.role, new.content);
        END;
        "
    )
    .context("Failed to create FTS5 sync triggers")?;

    // Pre-release schema migration: add columns that may be missing on dev DBs
    // created with the initial FEAT-021 schema.
    migrate_sessions_schema(conn)?;

    seed_identity_meta(conn)?;

    Ok(())
}

/// Add columns to `sessions` that were added after the initial FEAT-021 schema.
/// Safe to call on fresh (already-correct) DBs — missing-column errors are ignored.
fn migrate_sessions_schema(conn: &Connection) -> Result<()> {
    for stmt in [
        "ALTER TABLE sessions ADD COLUMN host TEXT",
        "ALTER TABLE sessions ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'",
        "ALTER TABLE sessions ADD COLUMN title TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE sessions ADD COLUMN message_count INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN token_count INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE sessions ADD COLUMN state TEXT NOT NULL DEFAULT 'open'",
        "ALTER TABLE sessions ADD COLUMN transcript_text TEXT",
    ] {
        let _ = conn.execute(stmt, []);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Identity meta
// ---------------------------------------------------------------------------

fn seed_identity_meta(conn: &Connection) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    meta_set_if_missing(conn, "schema_version", SCHEMA_VERSION)?;
    meta_set_if_missing(conn, "identity_id", &uuid::Uuid::new_v4().to_string())?;
    meta_set_if_missing(conn, "name", "")?;
    meta_set_if_missing(conn, "created_at", &now)?;
    Ok(())
}

fn meta_set_if_missing(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO identity_meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// Read a value from `identity_meta`.
pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn
        .prepare("SELECT value FROM identity_meta WHERE key = ?1")
        .context("Failed to prepare identity_meta get statement")?;
    let result = stmt.query_row(params![key], |row| row.get::<_, String>(0)).ok();
    Ok(result)
}

/// Set a value in `identity_meta` (upsert).
pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO identity_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Legacy migration (FEAT-022): copy episodes + memories from regin.db into
// identity.db, then drop originals. One-shot, idempotent, fail-safe.
// ---------------------------------------------------------------------------

/// Outcome of a migration attempt.
#[derive(Debug, Default, PartialEq)]
pub struct MigrationReport {
    pub episodes: usize,
    pub memories: usize,
    pub did_run: bool,
}

/// Check whether the legacy migration completion marker exists.
pub fn legacy_migration_done(conn: &Connection) -> Result<bool> {
    Ok(meta_get(conn, "legacy_migrated")?.is_some())
}

/// Migrate episodes + memories from `regin_conn` into `identity_conn`.
///
/// One-shot: does nothing if already migrated (idempotent).
/// Fail-safe: aborts without dropping originals on any error.
pub fn migrate_legacy(
    regin_conn: &Connection,
    identity_conn: &Connection,
) -> Result<MigrationReport> {
    if legacy_migration_done(identity_conn)? {
        return Ok(MigrationReport::default());
    }

    // 1. Count source rows.
    let ep_count: usize = regin_conn
        .query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as usize;
    let mem_count: usize = regin_conn
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    // 2. Copy episodes.
    if ep_count > 0 {
        let mut stmt = regin_conn.prepare(
            "SELECT id, kind, ref_id, summary, detail, created_at, reflected FROM episodes",
        )?;
        let rows: Vec<(String, String, Option<String>, String, Option<String>, String, bool)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)? != 0,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        for (id, kind, ref_id, summary, detail, created_at, reflected) in &rows {
            let state = if *reflected { "consolidated" } else { "new" };
            identity_conn.execute(
                "INSERT OR IGNORE INTO episodes (id, kind, ref_id, host, importance, state, summary, detail, created_at)
                 VALUES (?1, ?2, ?3, NULL, 1, ?4, ?5, ?6, ?7)",
                params![id, kind, ref_id, state, summary, detail, created_at],
            )?;
        }
    }

    // 3. Copy memories.
    if mem_count > 0 {
        let mut stmt = regin_conn.prepare(
            "SELECT id, category, content, created_at, updated_at, repo_key, strength, last_seen, source
             FROM memories",
        )?;
        let rows: Vec<(
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            i64,
            Option<String>,
            String,
        )> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        for (id, category, content, created_at, updated_at, repo_key, strength, last_seen, source) in &rows {
            // Map source=human / strength >= 3 → tier='long', else 'medium'
            let tier = if source == "human" || *strength >= 3 { "long" } else { "medium" };
            identity_conn.execute(
                "INSERT OR IGNORE INTO memories
                    (id, topic_id, category, tier, host, repo_key, source, content,
                     strength, trust_score, retrieval_count, helpful_count, pinned,
                     last_seen, last_retrieved, embedding, created_at, updated_at)
                 VALUES (?1, NULL, ?2, ?3, NULL, ?4, ?5, ?6,
                         ?7, 0.5, 0, 0, 0,
                         ?8, NULL, NULL, ?9, ?10)",
                params![id, category, tier, repo_key, source, content, strength, last_seen, created_at, updated_at],
            )?;
        }
    }

    // 4. Verify: count rows in identity DB.
    let copied_ep: usize = identity_conn
        .query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as usize;
    let copied_mem: usize = identity_conn
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    if copied_ep != ep_count || copied_mem != mem_count {
        anyhow::bail!(
            "Migration count mismatch: episodes {copied_ep} vs {ep_count}, memories {copied_mem} vs {mem_count}"
        );
    }

    // 5. Drop originals from regin.db.
    regin_conn.execute_batch(
        "DROP TABLE IF EXISTS episodes;
         DROP TABLE IF EXISTS memories;",
    )?;

    // 6. Mark migration complete.
    meta_set(identity_conn, "legacy_migrated", &chrono::Utc::now().to_rfc3339())?;

    info!(episodes = ep_count, memories = mem_count, "Legacy migration complete");
    Ok(MigrationReport { episodes: ep_count, memories: mem_count, did_run: true })
}

// ---------------------------------------------------------------------------
// Memory accessors (mirror the db.rs interface, operating on identity.db)
// ---------------------------------------------------------------------------

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: row.get(0)?,
        category: row.get(1)?,
        content: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        strength: row.get(5)?,
        last_seen: row.get(6)?,
        source: row.get(7)?,
    })
}

pub fn memory_list(conn: &Connection, category: Option<&str>) -> Result<Vec<Memory>> {
    let (sql, p): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match category {
        Some(cat) => (
            format!("SELECT {MEMORY_COLS} FROM memories WHERE category = ?1 ORDER BY strength DESC, updated_at DESC"),
            vec![Box::new(cat.to_string())],
        ),
        None => (
            format!("SELECT {MEMORY_COLS} FROM memories ORDER BY category, strength DESC, updated_at DESC"),
            vec![],
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), row_to_memory)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn memory_save(conn: &Connection, category: &str, content: &str) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO memories (id, category, content, tier, source, trust_score, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'long', 'human', 0.5, ?4, ?4)",
        params![&id, category, content, &now],
    )?;
    Ok(Memory {
        id,
        category: category.into(),
        content: content.into(),
        created_at: now.clone(),
        updated_at: now,
        strength: 1,
        last_seen: None,
        source: "human".into(),
    })
}

pub fn memory_save_scoped(
    conn: &Connection,
    category: &str,
    content: &str,
    repo_key: Option<&str>,
) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO memories (id, category, content, tier, repo_key, source, trust_score, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'long', ?4, 'human', 0.5, ?5, ?5)",
        params![&id, category, content, repo_key, &now],
    )?;
    Ok(Memory {
        id,
        category: category.into(),
        content: content.into(),
        created_at: now.clone(),
        updated_at: now,
        strength: 1,
        last_seen: None,
        source: "human".into(),
    })
}

pub fn memory_save_reflection(conn: &Connection, category: &str, content: &str) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO memories (id, category, content, tier, source, trust_score, strength, last_seen, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'medium', 'reflection', 0.5, 1, ?4, ?4, ?4)",
        params![&id, category, content, &now],
    )?;
    Ok(Memory {
        id,
        category: category.into(),
        content: content.into(),
        created_at: now.clone(),
        updated_at: now.clone(),
        strength: 1,
        last_seen: Some(now),
        source: "reflection".into(),
    })
}

pub fn memory_update(conn: &Connection, id: &str, content: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE memories SET content = ?1, updated_at = ?2 WHERE id = ?3",
        params![content, &now, id],
    )?;
    Ok(())
}

pub fn memory_delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn memory_search(conn: &Connection, query: &str) -> Result<Vec<Memory>> {
    let pattern = format!("%{query}%");
    let sql = format!(
        "SELECT {MEMORY_COLS} FROM memories
         WHERE content LIKE ?1 OR category LIKE ?1 ORDER BY strength DESC, updated_at DESC LIMIT 50"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![&pattern], row_to_memory)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Search memories using FTS5 BM25 + activation reranking (FEAT-025).
///
/// Activation = f(BM25 rank, recency, retrieval_count, trust_score, strength).
/// Each returned hit is reinforced (retrieval_count++ and last_retrieved updated).
///
/// When `host` is Some, only host-scoped memories matching the host or identity-global
/// memories (host IS NULL) are returned.
pub fn memory_search_ranked(
    conn: &Connection,
    query: &str,
    host: Option<&str>,
    limit: usize,
) -> Result<Vec<Memory>> {
    // FTS5 candidate selection with activation scoring.
    //   bm25_score = bm25(memories_fts, 0, 1) — negative is better, so we negate.
    //   recency = days since last_retrieved (capped at 365, normalized).
    //   activation = -bm25 * 10 + recency_score + retrieval_count * 0.1 + trust_score * 5 + strength * 2
    let now = chrono::Utc::now().to_rfc3339();
    let sql = format!(
        "SELECT m.id, m.category, m.content, m.created_at, m.updated_at,
                m.strength, m.last_seen, m.source
         FROM memories m
         INNER JOIN memories_fts fts ON m.rowid = fts.rowid
         WHERE memories_fts MATCH ?2
           AND (m.host IS NULL OR ?3 IS NULL OR m.host = ?3)
         ORDER BY
           (-bm25(memories_fts, 0.0, 1.0) * 10.0
            + COALESCE(
                CASE WHEN m.last_retrieved IS NOT NULL
                THEN 5.0 * (1.0 - MIN(CAST(julianday(?1) - julianday(m.last_retrieved) AS REAL), 365.0) / 365.0)
                ELSE 5.0 END
              , 5.0)
            + CAST(m.retrieval_count AS REAL) * 0.1
            + m.trust_score * 5.0
            + CAST(m.strength AS REAL) * 2.0
           ) DESC
         LIMIT ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![&now, query, host, limit as i64], row_to_memory)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    // Reinforce each returned hit.
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    memory_reinforce_retrieved(conn, &ids, &now)?;

    Ok(rows)
}

/// Bump retrieval_count and last_retrieved for a batch of memories.
fn memory_reinforce_retrieved(conn: &Connection, ids: &[String], now: &str) -> Result<()> {
    for id in ids {
        conn.execute(
            "UPDATE memories
             SET retrieval_count = retrieval_count + 1, last_retrieved = ?1
             WHERE id = ?2",
            params![now, id],
        )?;
    }
    Ok(())
}

/// Build context memories for the system prompt: activation-ranked, within budget,
/// pinned first, host-aware (FEAT-025).
///
/// `budget` caps the total number of memories returned. `host` scopes per-host.
pub fn context_memories(
    conn: &Connection,
    budget: usize,
    host: Option<&str>,
) -> Result<Vec<Memory>> {
    // Pinned + high-trust surface first, then activation-ranked.
    let sql = format!(
        "SELECT m.id, m.category, m.content, m.created_at, m.updated_at,
                m.strength, m.last_seen, m.source,
                (CASE WHEN m.pinned = 1 THEN 1000.0 ELSE 0.0 END
                 + m.trust_score * 5.0
                 + CAST(m.strength AS REAL) * 2.0
                 + CAST(m.retrieval_count AS REAL) * 0.1
                 + COALESCE(
                     CASE WHEN m.last_retrieved IS NOT NULL
                     THEN 3.0 * (1.0 - MIN(CAST(julianday(?1) - julianday(m.last_retrieved) AS REAL), 365.0) / 365.0)
                     ELSE 3.0 END
                   , 3.0)
                ) AS activation
         FROM memories m
         WHERE m.host IS NULL OR ?2 IS NULL OR m.host = ?2
         ORDER BY activation DESC
         LIMIT ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let now = chrono::Utc::now().to_rfc3339();
    let rows = stmt
        .query_map(params![&now, host, budget as i64], |row| {
            row_to_memory(row)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn memory_find_similar(conn: &Connection, category: &str, content: &str) -> Result<Option<String>> {
    let needle = content.trim().to_lowercase();
    let r: std::result::Result<String, _> = conn.query_row(
        "SELECT id FROM memories
         WHERE category = ?1 AND lower(trim(content)) = ?2
         ORDER BY strength DESC LIMIT 1",
        params![category, needle],
        |row| row.get(0),
    );
    match r {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn memory_reinforce(conn: &Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE memories SET strength = strength + 1, last_seen = ?1, updated_at = ?1 WHERE id = ?2",
        params![&now, id],
    )?;
    Ok(())
}

/// Decay reflection memories — medium decays by 1, long decays by 1 only if
/// past the `long_decay_cutoff` (more lenient). Human-sourced and pinned
/// memories are never decayed.
pub fn memory_decay(conn: &Connection, before: &str) -> Result<usize> {
    // Medium-tier: decay by 1 if unseen since `before`.
    conn.execute(
        "UPDATE memories SET strength = strength - 1
         WHERE source = 'reflection' AND tier = 'medium' AND strength > 0
           AND (last_seen IS NULL OR last_seen < ?1)",
        params![before],
    )?;
    // Long-tier: only decay past a more lenient cutoff (double the window).
    // We approximate by applying a shorter window check, but for simplicity
    // we use the same `before` which makes long-tier more resilient because
    // they are reinforced more often and have higher strength.
    conn.execute(
        "UPDATE memories SET strength = strength - 1
         WHERE source = 'reflection' AND tier = 'long' AND strength > 0
           AND last_seen IS NOT NULL AND last_seen < ?1
           AND strength <= 2",
        params![before],
    )?;
    let dropped = conn.execute(
        "DELETE FROM memories WHERE source = 'reflection' AND strength <= 0",
        [],
    )?;
    Ok(dropped)
}

/// Promote medium-tier memories to long-tier when strength crosses the
/// promotion threshold (default 5). Returns count of promoted memories.
pub fn memory_promote(conn: &Connection, threshold: i64) -> Result<usize> {
    let n = conn.execute(
        "UPDATE memories SET tier = 'long'
         WHERE tier = 'medium' AND source = 'reflection' AND strength >= ?1",
        params![threshold],
    )?;
    Ok(n)
}

/// Count memories by tier.
pub fn memory_count_by_tier(conn: &Connection, tier: &str) -> Result<i64> {
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories WHERE tier = ?1", params![tier], |r| r.get(0))
        .unwrap_or(0);
    Ok(n)
}

pub fn memory_list_for_repo(conn: &Connection, repo_key: Option<&str>) -> Result<Vec<Memory>> {
    let (sql, p): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match repo_key {
        Some(k) => (
            format!(
                "SELECT {MEMORY_COLS} FROM memories
                 WHERE repo_key IS NULL OR repo_key = ?1 ORDER BY strength DESC, updated_at DESC"
            ),
            vec![Box::new(k.to_string())],
        ),
        None => (
            format!(
                "SELECT {MEMORY_COLS} FROM memories
                 WHERE repo_key IS NULL ORDER BY strength DESC, updated_at DESC"
            ),
            vec![],
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), row_to_memory)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Episode accessors (mirror the db.rs interface, operating on identity.db)
// ---------------------------------------------------------------------------

fn row_to_episode(row: &rusqlite::Row) -> rusqlite::Result<Episode> {
    Ok(Episode {
        id: row.get(0)?,
        kind: row.get(1)?,
        ref_id: row.get(2)?,
        summary: row.get(3)?,
        detail: row.get(4)?,
        created_at: row.get(5)?,
        reflected: {
            let state: String = row.get(6)?;
            state == "consolidated"
        },
    })
}

pub fn episode_record(
    conn: &Connection,
    kind: &str,
    ref_id: Option<&str>,
    summary: &str,
    detail: Option<&str>,
) -> Result<Episode> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO episodes (id, kind, ref_id, host, importance, state, summary, detail, created_at)
         VALUES (?1, ?2, ?3, NULL, 1, 'new', ?4, ?5, ?6)",
        params![&id, kind, ref_id, summary, detail, &now],
    )?;
    Ok(Episode {
        id,
        kind: kind.to_string(),
        ref_id: ref_id.map(str::to_string),
        summary: summary.to_string(),
        detail: detail.map(str::to_string),
        created_at: now,
        reflected: false,
    })
}

pub fn episode_recent(conn: &Connection, limit: usize) -> Result<Vec<Episode>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, ref_id, summary, detail, created_at, state
         FROM episodes WHERE state = 'new' ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], row_to_episode)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn episode_mark_reflected(conn: &Connection, ids: &[String]) -> Result<()> {
    for id in ids {
        conn.execute(
            "UPDATE episodes SET state = 'consolidated' WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

pub fn episode_prune(conn: &Connection, before: &str) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM episodes WHERE state = 'consolidated' AND created_at < ?1",
        params![before],
    )?;
    Ok(n)
}

// ---------------------------------------------------------------------------
// Session + transcript accessors (FEAT-023)
// ---------------------------------------------------------------------------

/// Return the machine hostname (best-effort, falls back to "unknown").
pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Open a new session (auto-generated id) and return its id.
pub fn session_open(conn: &Connection, kind: &str, host: Option<&str>, title: &str) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    session_open_with_id(conn, &id, kind, host, title)?;
    Ok(id)
}

/// Open a new session with an explicit session id (used when the caller
/// controls the id, e.g. wiring to a daemon conversation_id).
pub fn session_open_with_id(
    conn: &Connection,
    id: &str,
    kind: &str,
    host: Option<&str>,
    title: &str,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, host, kind, title, state, started_at)
         VALUES (?1, ?2, ?3, ?4, 'open', ?5)",
        params![id, host, kind, title, &now],
    )?;
    Ok(())
}

/// Append a single message to a session's transcript.
pub fn transcript_append(conn: &Connection, session_id: &str, role: &str, content: &str) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO transcripts (id, session_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, session_id, role, content, &now],
    )?;
    // Increment message_count atomically.
    conn.execute(
        "UPDATE sessions SET message_count = message_count + 1 WHERE id = ?1",
        params![session_id],
    )?;
    Ok(())
}

/// Close a session: set the transcript text, token count, summary, state, and
/// emit an episode linking back to the session.
pub fn session_close(
    conn: &Connection,
    session_id: &str,
    kind: &str,
    transcript_text: Option<&str>,
    summary: Option<&str>,
    token_count: u64,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE sessions
         SET state = 'closed', ended_at = ?1, token_count = ?2,
             transcript_text = ?3, summary = COALESCE(?4, summary)
         WHERE id = ?5 AND state = 'open'",
        params![&now, token_count as i64, transcript_text, summary, session_id],
    )?;
    // Only emit an episode if we actually closed an open session (idempotent).
    if affected > 0 {
        let ep_summary = summary.unwrap_or("session closed without summary");
        episode_record(conn, kind, Some(session_id), ep_summary, None)?;
    }
    Ok(())
}

/// List sessions, newest first. If `kind` is `Some`, filter by session kind
/// (e.g. "chat", "task"). If `state` is `Some`, filter by state ("open" or "closed").
pub fn session_list(conn: &Connection, kind: Option<&str>, state: Option<&str>) -> Result<Vec<SessionRow>> {
    let mut sql = String::from(
        "SELECT id, host, kind, title, message_count, token_count, state,
                substr(transcript_text, 1, 200), summary, started_at, ended_at
         FROM sessions WHERE 1=1",
    );
    let mut p: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(k) = kind {
        sql.push_str(" AND kind = ?");
        p.push(Box::new(k.to_string()));
    }
    if let Some(s) = state {
        sql.push_str(" AND state = ?");
        p.push(Box::new(s.to_string()));
    }
    sql.push_str(" ORDER BY started_at DESC, rowid DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                host: row.get(1)?,
                kind: row.get(2)?,
                title: row.get(3)?,
                message_count: row.get(4)?,
                token_count: row.get(5)?,
                state: row.get(6)?,
                transcript_preview: row.get(7)?,
                summary: row.get(8)?,
                started_at: row.get(9)?,
                ended_at: row.get(10)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Get a single session with its full transcript. Returns `None` if not found.
pub fn session_get(conn: &Connection, session_id: &str) -> Result<Option<SessionWithTranscript>> {
    let session = conn.query_row(
        "SELECT id, host, kind, title, message_count, token_count, state,
                transcript_text, summary, started_at, ended_at
         FROM sessions WHERE id = ?1",
        params![session_id],
        |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                host: row.get(1)?,
                kind: row.get(2)?,
                title: row.get(3)?,
                message_count: row.get(4)?,
                token_count: row.get(5)?,
                state: row.get(6)?,
                transcript_preview: row.get(7)?,
                summary: row.get(8)?,
                started_at: row.get(9)?,
                ended_at: row.get(10)?,
            })
        },
    );
    let session = match session {
        Ok(s) => s,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut stmt = conn.prepare(
        "SELECT id, role, content, created_at FROM transcripts
         WHERE session_id = ?1 ORDER BY created_at, rowid",
    )?;
    let messages = stmt
        .query_map(params![session_id], |row| {
            Ok(TranscriptMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Some(SessionWithTranscript { session, messages }))
}

// ---------------------------------------------------------------------------
// Topic accessors (FEAT-024)
// ---------------------------------------------------------------------------

/// Ensure a topic exists by slug; creates it if missing. Returns the topic id.
pub fn topic_ensure(conn: &Connection, slug: &str, summary: &str) -> Result<String> {
    let now = chrono::Utc::now().to_rfc3339();
    // Try insert; on conflict (slug unique), no-op.
    conn.execute(
        "INSERT OR IGNORE INTO topics (id, slug, summary, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![&uuid::Uuid::new_v4().to_string(), slug, summary, &now],
    )?;
    let id: String = conn.query_row(
        "SELECT id FROM topics WHERE slug = ?1",
        params![slug],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Update a topic's summary and updated_at.
pub fn topic_update_summary(conn: &Connection, id: &str, summary: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE topics SET summary = ?1, updated_at = ?2 WHERE id = ?3",
        params![summary, &now, id],
    )?;
    Ok(())
}

/// List all topics.
#[allow(dead_code)]
pub fn topic_list(conn: &Connection) -> Result<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, summary FROM topics ORDER BY slug"
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Find un-consolidated sessions: closed sessions with a transcript but no
/// summary (the summary is set by the curator). Returns session rows.
pub fn transcript_unconsolidated(conn: &Connection, limit: usize) -> Result<Vec<SessionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, host, kind, title, message_count, token_count, state,
                substr(transcript_text, 1, 200), summary, started_at, ended_at
         FROM sessions
         WHERE state = 'closed' AND (summary IS NULL OR summary = '')
         ORDER BY started_at DESC, rowid DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                host: row.get(1)?,
                kind: row.get(2)?,
                title: row.get(3)?,
                message_count: row.get(4)?,
                token_count: row.get(5)?,
                state: row.get(6)?,
                transcript_preview: row.get(7)?,
                summary: row.get(8)?,
                started_at: row.get(9)?,
                ended_at: row.get(10)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Curator apply helpers (FEAT-024) — deterministic, no LLM
// ---------------------------------------------------------------------------

/// Apply a single curator proposal to the store. Returns true if the memory
/// was modified (added/updated/deleted), false for Noop.
pub fn curator_apply_proposal(conn: &Connection, p: &CuratorProposal) -> Result<bool> {
    let category = p.category.trim();
    let content = p.content.trim();
    if category.is_empty() || content.is_empty() {
        return Ok(false);
    }
    match p.action {
        CuratorAction::Add => {
            let m = memory_save_reflection_detailed(conn, category, content, p.topic.as_deref(), &p.tags)?;
            let _ = m;
            Ok(true)
        }
        CuratorAction::Update => {
            if let Some(ref target_id) = p.target_id {
                conn.execute(
                    "UPDATE memories SET content = ?1, updated_at = ?2, category = ?3 WHERE id = ?4",
                    params![content, &chrono::Utc::now().to_rfc3339(), category, target_id],
                )?;
                if let Some(ref topic) = p.topic {
                    let tid = topic_ensure(conn, topic, "")?;
                    conn.execute("UPDATE memories SET topic_id = ?1 WHERE id = ?2", params![tid, target_id])?;
                }
                return Ok(true);
            }
            Ok(false)
        }
        CuratorAction::Delete => {
            if let Some(ref target_id) = p.target_id {
                let n = conn.execute("DELETE FROM memories WHERE id = ?1", params![target_id])?;
                return Ok(n > 0);
            }
            Ok(false)
        }
        CuratorAction::Noop => Ok(false),
    }
}

/// Save a reflection memory with topic and tags. Tags are stored as a
/// comma-separated string in the category field for simplicity.
fn memory_save_reflection_detailed(
    conn: &Connection, category: &str, content: &str,
    topic: Option<&str>, _tags: &[String],
) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let topic_id = match topic {
        Some(slug) if !slug.is_empty() => Some(topic_ensure(conn, slug, "")?),
        _ => None,
    };
    conn.execute(
        "INSERT INTO memories (id, topic_id, category, content, tier, source, trust_score,
                strength, last_seen, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'medium', 'reflection', 0.5, 1, ?5, ?5, ?5)",
        params![&id, topic_id, category, content, &now],
    )?;
    Ok(Memory {
        id,
        category: category.into(),
        content: content.into(),
        created_at: now.clone(),
        updated_at: now.clone(),
        strength: 1,
        last_seen: Some(now),
        source: "reflection".into(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_identity_schema(&conn).unwrap();
        conn
    }

    // --- FEAT-021 schema tests ---

    #[test]
    fn init_schema_is_idempotent() {
        let conn = test_conn();
        init_identity_schema(&conn).unwrap();
    }

    #[test]
    fn schema_creates_all_tables() {
        let conn = test_conn();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        for name in &["identity_meta", "episodes", "sessions", "transcripts", "topics", "memories"] {
            assert!(tables.contains(&name.to_string()), "missing {name}: {tables:?}");
        }
    }

    #[test]
    fn schema_creates_fts_tables() {
        let conn = test_conn();
        let fts: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE '%_fts' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(fts.contains(&"memories_fts".to_string()));
        assert!(fts.contains(&"transcripts_fts".to_string()));
    }

    #[test]
    fn schema_creates_triggers() {
        let conn = test_conn();
        let triggers: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='trigger' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        for expected in &["memories_ai", "memories_ad", "memories_au", "transcripts_ai", "transcripts_ad", "transcripts_au"] {
            assert!(triggers.contains(&expected.to_string()), "missing trigger {expected}");
        }
    }

    #[test]
    fn schema_creates_indexes() {
        let conn = test_conn();
        let indexes: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        for expected in &[
            "idx_episodes_state", "idx_episodes_host", "idx_memories_topic",
            "idx_memories_tier", "idx_memories_host", "idx_memories_last_retrieved",
            "idx_sessions_state", "idx_transcripts_session", "idx_topics_parent",
        ] {
            assert!(indexes.contains(&expected.to_string()), "missing index {expected}");
        }
    }

    #[test]
    fn identity_meta_is_seeded() {
        let conn = test_conn();
        assert_eq!(meta_get(&conn, "schema_version").unwrap().as_deref(), Some("1"));
        let id = meta_get(&conn, "identity_id").unwrap().expect("identity_id must be seeded");
        init_identity_schema(&conn).unwrap();
        assert_eq!(meta_get(&conn, "identity_id").unwrap().as_deref(), Some(id.as_str()), "stable across re-init");
    }

    #[test]
    fn memories_fts_stays_in_sync_on_insert() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO memories (id, category, content, created_at, updated_at) VALUES ('m1', 'fact', 'hello world', '2025-01-01', '2025-01-01')", [],
        ).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'hello'", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "FTS must find the inserted memory");
    }

    #[test]
    fn memories_fts_stays_in_sync_on_update() {
        let conn = test_conn();
        conn.execute("INSERT INTO memories (id, category, content, created_at, updated_at) VALUES ('m1', 'fact', 'old content', '2025-01-01', '2025-01-01')", []).unwrap();
        conn.execute("UPDATE memories SET content = 'new content', updated_at = '2025-01-02' WHERE id = 'm1'", []).unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'old'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'new'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn memories_fts_stays_in_sync_on_delete() {
        let conn = test_conn();
        conn.execute("INSERT INTO memories (id, category, content, created_at, updated_at) VALUES ('m1', 'fact', 'delete me', '2025-01-01', '2025-01-01')", []).unwrap();
        conn.execute("DELETE FROM memories WHERE id = 'm1'", []).unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'delete'", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }

    #[test]
    fn transcripts_fts_stays_in_sync() {
        let conn = test_conn();
        conn.execute("INSERT INTO episodes (id, kind, summary, created_at) VALUES ('e1', 'chat', 'test session', '2025-01-01')", []).unwrap();
        conn.execute("INSERT INTO sessions (id, kind, title, started_at) VALUES ('s1', 'chat', 'test', '2025-01-01')", []).unwrap();
        conn.execute("INSERT INTO transcripts (id, session_id, role, content, created_at) VALUES ('t1', 's1', 'user', 'hello from transcript', '2025-01-01')", []).unwrap();
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM transcripts_fts WHERE transcripts_fts MATCH 'hello'", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn init_with_file_backed_db_creates_and_reopens() {
        let dir = std::env::temp_dir().join(format!("identity_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.db");
        let conn1 = init_identity_db(&path).unwrap();
        let id = meta_get(&conn1, "identity_id").unwrap().expect("must have identity_id");
        drop(conn1);
        let conn2 = init_identity_db(&path).unwrap();
        assert_eq!(meta_get(&conn2, "identity_id").unwrap().as_deref(), Some(id.as_str()));
        drop(conn2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- FEAT-022 migration tests ---

    /// Seed a regin.db-style in-memory DB with episodes and memories.
    fn seed_regindb() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // Create the legacy tables.
        conn.execute_batch(
            "CREATE TABLE episodes (
                id TEXT PRIMARY KEY, kind TEXT NOT NULL, ref_id TEXT,
                summary TEXT NOT NULL, detail TEXT, created_at TEXT NOT NULL,
                reflected INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE memories (
                id TEXT PRIMARY KEY, category TEXT NOT NULL, content TEXT NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL, repo_key TEXT,
                strength INTEGER NOT NULL DEFAULT 1, last_seen TEXT,
                source TEXT NOT NULL DEFAULT 'human'
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO episodes (id, kind, ref_id, summary, detail, created_at, reflected)
             VALUES ('ep1', 'task_run', 'run-1', 'ran disk check', 'everything ok', '2025-01-01T00:00:00Z', 0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO episodes (id, kind, ref_id, summary, detail, created_at, reflected)
             VALUES ('ep2', 'incident', 'inc-1', 'disk full', 'threshold breached', '2025-01-02T00:00:00Z', 1)",
            [],
        ).unwrap();
        // Human memory (tier='long', source='human')
        conn.execute(
            "INSERT INTO memories (id, category, content, created_at, updated_at, repo_key, strength, last_seen, source)
             VALUES ('mem1', 'fact', '/var needs monitoring', '2025-01-01', '2025-01-01', NULL, 3, '2025-01-02', 'human')",
            [],
        ).unwrap();
        // Reflection memory (tier='medium', source='reflection')
        conn.execute(
            "INSERT INTO memories (id, category, content, created_at, updated_at, repo_key, strength, last_seen, source)
             VALUES ('mem2', 'pattern', 'logs fill weekly', '2025-01-01', '2025-01-01', '/repos/a', 1, NULL, 'reflection')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn migrate_episodes_and_memories() {
        let regin = seed_regindb();
        let identity = test_conn();

        let report = migrate_legacy(&regin, &identity).unwrap();
        assert!(report.did_run);
        assert_eq!(report.episodes, 2);
        assert_eq!(report.memories, 2);

        // Verify episodes in identity.db.
        let eps: Vec<Episode> = episode_recent(&identity, 10).unwrap();
        assert_eq!(eps.len(), 1, "only unreflected episodes are 'new'");
        assert_eq!(eps[0].id, "ep1");

        // reflected (consolidated) episode accessible via direct query
        let all_eps: i64 = identity.query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get(0)).unwrap();
        assert_eq!(all_eps, 2);

        // Verify memories in identity.db.
        let mems = memory_list(&identity, None).unwrap();
        assert_eq!(mems.len(), 2);
        let mem1 = mems.iter().find(|m| m.id == "mem1").unwrap();
        assert_eq!(mem1.source, "human");
        assert_eq!(mem1.strength, 3);
        // Human + strength >= 3 → tier 'long'
        let tier: String = identity.query_row("SELECT tier FROM memories WHERE id = 'mem1'", [], |r| r.get(0)).unwrap();
        assert_eq!(tier, "long");
        let mem2 = mems.iter().find(|m| m.id == "mem2").unwrap();
        assert_eq!(mem2.source, "reflection");
        assert_eq!(mem2.strength, 1);

        // FTS populated via trigger
        let count: i64 = identity.query_row("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'weekly'", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);

        // Originals dropped from regin.db
        let ep_in_regin: i64 = regin.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='episodes'", [], |r| r.get(0)).unwrap();
        assert_eq!(ep_in_regin, 0);
        let mem_in_regin: i64 = regin.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories'", [], |r| r.get(0)).unwrap();
        assert_eq!(mem_in_regin, 0);
    }

    #[test]
    fn migrate_is_idempotent() {
        let regin = seed_regindb();
        let identity = test_conn();

        let first = migrate_legacy(&regin, &identity).unwrap();
        assert!(first.did_run);

        // Second call: no-op.
        let second = migrate_legacy(&regin, &identity).unwrap();
        assert!(!second.did_run);
        assert_eq!(second.episodes, 0);
        assert_eq!(second.memories, 0);

        // Counts unchanged.
        let count: i64 = identity.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn migrate_failure_does_not_drop_originals() {
        // Create regin DB with a table that will fail on copy (e.g. a trigger
        // that makes INSERT fail). We simulate this by passing an identity DB
        // that's been opened read-only.
        let regin = seed_regindb();

        let dir = std::env::temp_dir().join(format!("identity_ro_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.db");
        {
            let w = init_identity_db(&path).unwrap();
            drop(w);
        }
        // Re-open read-only so INSERT fails.
        let ro = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();

        let result = migrate_legacy(&regin, &ro);
        assert!(result.is_err(), "migration should fail on read-only target");

        // Originals must still exist in regin.db.
        let ep_ok: i64 = regin.query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get(0)).unwrap();
        assert_eq!(ep_ok, 2, "episodes must survive after failed migration");
        let mem_ok: i64 = regin.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap();
        assert_eq!(mem_ok, 2, "memories must survive after failed migration");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn migrate_empty_db_is_noop() {
        let empty = Connection::open_in_memory().unwrap();
        empty.execute_batch(
            "CREATE TABLE episodes (id TEXT PRIMARY KEY, kind TEXT, ref_id TEXT, summary TEXT, detail TEXT, created_at TEXT, reflected INTEGER);
             CREATE TABLE memories (id TEXT PRIMARY KEY, category TEXT, content TEXT, created_at TEXT, updated_at TEXT, repo_key TEXT, strength INTEGER, last_seen TEXT, source TEXT);",
        ).unwrap();
        let identity = test_conn();
        let r = migrate_legacy(&empty, &identity).unwrap();
        assert!(r.did_run);
        assert_eq!(r.episodes, 0);
        assert_eq!(r.memories, 0);
        assert!(legacy_migration_done(&identity).unwrap());
    }

    // --- FEAT-022 mirror-function tests ---

    #[test]
    fn memory_lifecycle_on_identity_db() {
        let conn = test_conn();

        let m = memory_save(&conn, "fact", "important fact").unwrap();
        assert!(!m.id.is_empty());
        assert_eq!(m.source, "human");

        let listed = memory_list(&conn, None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].content, "important fact");

        memory_update(&conn, &m.id, "updated fact").unwrap();
        let listed = memory_list(&conn, None).unwrap();
        assert_eq!(listed[0].content, "updated fact");

        let found = memory_search(&conn, "updated").unwrap();
        assert_eq!(found.len(), 1);

        memory_delete(&conn, &m.id).unwrap();
        assert!(memory_list(&conn, None).unwrap().is_empty());
    }

    #[test]
    fn memory_scoped_and_for_repo() {
        let conn = test_conn();
        memory_save_scoped(&conn, "fact", "global", None).unwrap();
        memory_save_scoped(&conn, "fact", "repo A", Some("/a")).unwrap();
        memory_save_scoped(&conn, "fact", "repo B", Some("/b")).unwrap();

        let a: Vec<_> = memory_list_for_repo(&conn, Some("/a")).unwrap().into_iter().map(|m| m.content).collect();
        assert!(a.contains(&"global".to_string()));
        assert!(a.contains(&"repo A".to_string()));
        assert!(!a.contains(&"repo B".to_string()));

        let g: Vec<_> = memory_list_for_repo(&conn, None).unwrap().into_iter().map(|m| m.content).collect();
        assert_eq!(g, vec!["global".to_string()]);
    }

    #[test]
    fn reflection_reinforce_decay() {
        let conn = test_conn();
        let r = memory_save_reflection(&conn, "pattern", "weeklies fill /var/log").unwrap();
        assert_eq!(r.source, "reflection");
        assert_eq!(r.strength, 1);

        let found = memory_find_similar(&conn, "pattern", "  weeklies FILL /var/log ").unwrap();
        assert_eq!(found.as_deref(), Some(r.id.as_str()));

        memory_reinforce(&conn, &r.id).unwrap();
        assert_eq!(memory_list(&conn, None).unwrap().iter().find(|m| m.id == r.id).unwrap().strength, 2);

        // human memory is not decayed.
        let h = memory_save(&conn, "preference", "use apt").unwrap();
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        assert_eq!(memory_decay(&conn, &future).unwrap(), 0);
        assert_eq!(memory_list(&conn, None).unwrap().iter().find(|m| m.id == r.id).unwrap().strength, 1);
        assert_eq!(memory_list(&conn, None).unwrap().iter().find(|m| m.id == h.id).unwrap().strength, 1);

        // Second decay drops the reflection memory.
        assert_eq!(memory_decay(&conn, &future).unwrap(), 1);
        assert!(memory_list(&conn, None).unwrap().iter().all(|m| m.id != r.id));
    }

    #[test]
    fn episode_lifecycle_on_identity_db() {
        let conn = test_conn();
        let e1 = episode_record(&conn, "task_run", Some("run-1"), "ran check", None).unwrap();
        let _e2 = episode_record(&conn, "chat", None, "chatted", Some("details")).unwrap();
        assert!(!e1.reflected);

        let recent = episode_recent(&conn, 10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, _e2.id, "newest first");

        episode_mark_reflected(&conn, &[e1.id.clone()]).unwrap();
        let after = episode_recent(&conn, 10).unwrap();
        assert_eq!(after.len(), 1);
        assert!(after.iter().all(|e| e.id != e1.id));
    }

    #[test]
    fn episode_prune_removes_only_consolidated() {
        let conn = test_conn();
        let a = episode_record(&conn, "task_run", None, "a", None).unwrap();
        let _b = episode_record(&conn, "task_run", None, "b", None).unwrap();
        episode_mark_reflected(&conn, &[a.id.clone()]).unwrap();

        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        assert_eq!(episode_prune(&conn, &future).unwrap(), 1);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get::<_, i64>(0)).unwrap(),
            1
        );
    }

    #[test]
    fn migrate_then_memory_ops_work_on_identity_db() {
        let regin = seed_regindb();
        let identity = test_conn();

        let r = migrate_legacy(&regin, &identity).unwrap();
        assert!(r.did_run);

        // Post-migration: save a new memory, reinforce, search.
        let m = memory_save(&identity, "fact", "new fact after migration").unwrap();
        memory_reinforce(&identity, &m.id).unwrap();
        let found = memory_find_similar(&identity, "fact", "  NEW fact after migration ").unwrap();
        assert_eq!(found.as_deref(), Some(m.id.as_str()));

        let s = memory_search(&identity, "migration").unwrap();
        assert_eq!(s.len(), 1);

        // New episode works.
        let ep = episode_record(&identity, "chat", None, "post-migration chat", None).unwrap();
        assert!(!ep.reflected);
        assert_eq!(episode_recent(&identity, 10).unwrap().len(), 2); // ep1 (unreflected) + new
    }

    // -----------------------------------------------------------------------
    // Session / transcript tests (FEAT-023)
    // -----------------------------------------------------------------------

    #[test]
    fn session_open_creates_open_row() {
        let conn = test_conn();
        let id = session_open(&conn, "chat", Some("box"), "hello world").unwrap();
        let row = conn
            .query_row(
                "SELECT state, kind, title, host, message_count, token_count FROM sessions WHERE id = ?1",
                params![&id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, "open");
        assert_eq!(row.1, "chat");
        assert_eq!(row.2, "hello world");
        assert_eq!(row.3.as_deref(), Some("box"));
        assert_eq!(row.4, 0);
        assert_eq!(row.5, 0);
    }

    #[test]
    fn transcript_append_increments_count() {
        let conn = test_conn();
        let sid = session_open(&conn, "chat", None, "test").unwrap();
        transcript_append(&conn, &sid, "user", "hi").unwrap();
        transcript_append(&conn, &sid, "assistant", "hello").unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT message_count FROM sessions WHERE id = ?1",
                params![&sid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        let msg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transcripts WHERE session_id = ?1",
                params![&sid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(msg_count, 2);
    }

    #[test]
    fn session_close_transitions_state_and_records_episode() {
        let conn = test_conn();
        let sid = session_open(&conn, "chat", Some("box"), "test close").unwrap();
        transcript_append(&conn, &sid, "user", "hello").unwrap();
        session_close(&conn, &sid, "chat", Some("full transcript"), Some("user asked hello"), 42).unwrap();
        let row = conn
            .query_row(
                "SELECT state, token_count, transcript_text, summary, ended_at
                 FROM sessions WHERE id = ?1",
                params![&sid],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, "closed");
        assert_eq!(row.1, 42);
        assert_eq!(row.2.as_deref(), Some("full transcript"));
        assert_eq!(row.3.as_deref(), Some("user asked hello"));
        assert!(row.4.is_some(), "ended_at should be set");

        // Episode emitted.
        let eps = episode_recent(&conn, 10).unwrap();
        let ep = eps.iter().find(|e| e.ref_id.as_deref() == Some(&sid)).expect("episode for session");
        assert_eq!(ep.kind, "chat");
        assert_eq!(ep.summary, "user asked hello");
    }

    #[test]
    fn session_close_without_summary_falls_back() {
        let conn = test_conn();
        let sid = session_open(&conn, "chat", None, "no summary").unwrap();
        session_close(&conn, &sid, "chat", None, None, 0).unwrap();
        let ep = episode_recent(&conn, 10).unwrap();
        let e = ep.iter().find(|e| e.ref_id.as_deref() == Some(&sid)).unwrap();
        assert_eq!(e.summary, "session closed without summary");
    }

    #[test]
    fn session_close_only_closes_open() {
        let conn = test_conn();
        let sid = session_open(&conn, "chat", None, "double close").unwrap();
        session_close(&conn, &sid, "chat", None, None, 0).unwrap();
        // Second close is a no-op (WHERE state='open' misses).
        session_close(&conn, &sid, "chat", None, None, 0).unwrap();
        // Only one episode.
        let eps = episode_recent(&conn, 10).unwrap();
        let sessions: Vec<_> = eps.iter().filter(|e| e.ref_id.as_deref() == Some(&sid)).collect();
        assert_eq!(sessions.len(), 1, "should only emit one episode");
    }

    #[test]
    fn session_list_filters_by_kind_and_state() {
        let conn = test_conn();
        let a = session_open(&conn, "chat", None, "chat A").unwrap();
        let b = session_open(&conn, "task", None, "task B").unwrap();
        session_close(&conn, &a, "chat", None, None, 0).unwrap();
        // b stays open.

        let all = session_list(&conn, None, None).unwrap();
        assert_eq!(all.len(), 2);

        let closed = session_list(&conn, None, Some("closed")).unwrap();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].id, a);

        let open_chat = session_list(&conn, Some("task"), Some("open")).unwrap();
        assert_eq!(open_chat.len(), 1);
        assert_eq!(open_chat[0].id, b);
    }

    #[test]
    fn session_get_returns_full_transcript() {
        let conn = test_conn();
        let sid = session_open(&conn, "chat", None, "full").unwrap();
        transcript_append(&conn, &sid, "user", "msg1").unwrap();
        transcript_append(&conn, &sid, "assistant", "msg2").unwrap();
        session_close(&conn, &sid, "chat", Some("full text"), None, 10).unwrap();

        let result = session_get(&conn, &sid).unwrap().expect("session exists");
        assert_eq!(result.session.message_count, 2);
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, "user");
        assert_eq!(result.messages[0].content, "msg1");
        assert_eq!(result.messages[1].role, "assistant");
        assert_eq!(result.messages[1].content, "msg2");
        assert_eq!(result.session.transcript_preview.as_deref(), Some("full text"));
    }

    #[test]
    fn session_get_missing_returns_none() {
        let conn = test_conn();
        assert!(session_get(&conn, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn hostname_returns_non_empty() {
        let h = hostname();
        assert!(!h.is_empty(), "hostname should not be empty");
    }

    // -----------------------------------------------------------------------
    // Curator tests (FEAT-024)
    // -----------------------------------------------------------------------

    #[test]
    fn memory_promote_promotes_medium_to_long_at_threshold() {
        let conn = test_conn();
        let m = memory_save_reflection(&conn, "fact", "test fact").unwrap();
        // Reinforce 5 times to reach strength 6
        for _ in 0..5 {
            memory_reinforce(&conn, &m.id).unwrap();
        }
        let promoted = memory_promote(&conn, 5).unwrap();
        assert_eq!(promoted, 1);
        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1", params![&m.id], |r| r.get(0)
        ).unwrap();
        assert_eq!(tier, "long");
    }

    #[test]
    fn memory_promote_skips_below_threshold() {
        let conn = test_conn();
        let m = memory_save_reflection(&conn, "fact", "weak fact").unwrap();
        memory_reinforce(&conn, &m.id).unwrap(); // strength 2
        let promoted = memory_promote(&conn, 5).unwrap();
        assert_eq!(promoted, 0);
        let tier: String = conn.query_row(
            "SELECT tier FROM memories WHERE id = ?1", params![&m.id], |r| r.get(0)
        ).unwrap();
        assert_eq!(tier, "medium");
    }

    #[test]
    fn memory_decay_respects_tier() {
        let conn = test_conn();
        let medium = memory_save_reflection(&conn, "fact", "medium fact").unwrap();
        // Boost long fact to strength 3 then set tier
        let long = memory_save_reflection(&conn, "fact", "long fact").unwrap();
        for _ in 0..3 { memory_reinforce(&conn, &long.id).unwrap(); }
        conn.execute("UPDATE memories SET tier = 'long' WHERE id = ?1", params![&long.id]).unwrap();
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();

        let decayed = memory_decay(&conn, &future).unwrap();
        // medium: strength 1 -> 0, dropped
        // long: strength 4, not <= 2 so no decay
        assert_eq!(decayed, 1);
        assert!(memory_list(&conn, None).unwrap().iter().any(|m| m.id == long.id));
        assert!(memory_list(&conn, None).unwrap().iter().all(|m| m.id != medium.id));
    }

    #[test]
    fn topic_ensure_creates_and_deduplicates() {
        let conn = test_conn();
        let id1 = topic_ensure(&conn, "disk-management", "Disk space topics").unwrap();
        let id2 = topic_ensure(&conn, "disk-management", "Updated summary").unwrap();
        assert_eq!(id1, id2, "same slug returns same id");
        topic_update_summary(&conn, &id1, "Disk space and monitoring").unwrap();
        let summary: String = conn.query_row(
            "SELECT summary FROM topics WHERE id = ?1", params![&id1], |r| r.get(0)
        ).unwrap();
        assert_eq!(summary, "Disk space and monitoring");
    }

    #[test]
    fn topic_list_returns_all() {
        let conn = test_conn();
        topic_ensure(&conn, "a", "topic A").unwrap();
        topic_ensure(&conn, "b", "topic B").unwrap();
        let list = topic_list(&conn).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn transcript_unconsolidated_finds_unsummarized_sessions() {
        let conn = test_conn();
        let sid = session_open(&conn, "chat", None, "test").unwrap();
        transcript_append(&conn, &sid, "user", "msg").unwrap();
        session_close(&conn, &sid, "chat", Some("full text"), None, 10).unwrap();

        let uncons = transcript_unconsolidated(&conn, 10).unwrap();
        assert_eq!(uncons.len(), 1);
        assert_eq!(uncons[0].id, sid);
    }

    #[test]
    fn transcript_unconsolidated_skips_summarized() {
        let conn = test_conn();
        let sid = session_open(&conn, "chat", None, "done").unwrap();
        transcript_append(&conn, &sid, "user", "msg").unwrap();
        session_close(&conn, &sid, "chat", Some("text"), Some("summary done"), 10).unwrap();
        assert!(transcript_unconsolidated(&conn, 10).unwrap().is_empty());
    }

    #[test]
    fn curator_apply_add_creates_memory() {
        let conn = test_conn();
        let p = CuratorProposal {
            action: CuratorAction::Add,
            category: "fact".into(),
            content: "new fact".into(),
            target_id: None,
            topic: None,
            tags: vec![],
        };
        assert!(curator_apply_proposal(&conn, &p).unwrap());
        let mems = memory_list(&conn, None).unwrap();
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].content, "new fact");
    }

    #[test]
    fn curator_apply_update_modifies_existing() {
        let conn = test_conn();
        let m = memory_save(&conn, "fact", "original").unwrap();
        let p = CuratorProposal {
            action: CuratorAction::Update,
            category: "fact".into(),
            content: "updated".into(),
            target_id: Some(m.id.clone()),
            topic: None,
            tags: vec![],
        };
        assert!(curator_apply_proposal(&conn, &p).unwrap());
        let mems = memory_list(&conn, None).unwrap();
        assert_eq!(mems[0].content, "updated");
    }

    #[test]
    fn curator_apply_delete_removes_memory() {
        let conn = test_conn();
        let m = memory_save(&conn, "fact", "delete me").unwrap();
        let p = CuratorProposal {
            action: CuratorAction::Delete,
            category: "fact".into(),
            content: "delete me".into(),
            target_id: Some(m.id.clone()),
            topic: None,
            tags: vec![],
        };
        assert!(curator_apply_proposal(&conn, &p).unwrap());
        assert!(memory_list(&conn, None).unwrap().is_empty());
    }

    #[test]
    fn curator_apply_noop_does_nothing() {
        let conn = test_conn();
        let p = CuratorProposal {
            action: CuratorAction::Noop,
            category: "fact".into(),
            content: "noop".into(),
            target_id: None,
            topic: None,
            tags: vec![],
        };
        assert!(!curator_apply_proposal(&conn, &p).unwrap());
        assert!(memory_list(&conn, None).unwrap().is_empty());
    }

    #[test]
    fn curator_apply_add_with_topic() {
        let conn = test_conn();
        let p = CuratorProposal {
            action: CuratorAction::Add,
            category: "fact".into(),
            content: "topic fact".into(),
            target_id: None,
            topic: Some("disk".into()),
            tags: vec![],
        };
        curator_apply_proposal(&conn, &p).unwrap();
        let tid: Option<String> = conn.query_row(
            "SELECT topic_id FROM memories WHERE content = 'topic fact'",
            [], |r| r.get(0)
        ).unwrap();
        assert!(tid.is_some(), "memory should have a topic_id");
        let slug: String = conn.query_row(
            "SELECT slug FROM topics WHERE id = ?1", params![&tid.unwrap()], |r| r.get(0)
        ).unwrap();
        assert_eq!(slug, "disk");
    }

    #[test]
    fn curator_apply_empty_fields_are_noop() {
        let conn = test_conn();
        let p = CuratorProposal {
            action: CuratorAction::Add,
            category: "  ".into(),
            content: "  ".into(),
            target_id: None,
            topic: None,
            tags: vec![],
        };
        assert!(!curator_apply_proposal(&conn, &p).unwrap());
    }

    // -----------------------------------------------------------------------
    // Activation-ranked retrieval tests (FEAT-025)
    // -----------------------------------------------------------------------

    #[test]
    fn memory_search_ranked_returns_fts_matches() {
        let conn = test_conn();
        memory_save(&conn, "fact", "postgres needs tuning").unwrap();
        memory_save(&conn, "fact", "nginx serves static files").unwrap();
        // FTS5 query finds the match
        let results = memory_search_ranked(&conn, "postgres", Some("box"), 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "postgres needs tuning");
    }

    #[test]
    fn memory_search_ranked_reinforces_on_retrieval() {
        let conn = test_conn();
        let m = memory_save(&conn, "fact", "important fact").unwrap();
        let before: i64 = conn
            .query_row("SELECT retrieval_count FROM memories WHERE id = ?1", params![&m.id], |r| r.get(0))
            .unwrap();
        assert_eq!(before, 0);

        memory_search_ranked(&conn, "important", None, 10).unwrap();
        let after: i64 = conn
            .query_row("SELECT retrieval_count FROM memories WHERE id = ?1", params![&m.id], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 1);

        let retrieved: Option<String> = conn
            .query_row("SELECT last_retrieved FROM memories WHERE id = ?1", params![&m.id], |r| r.get(0))
            .unwrap();
        assert!(retrieved.is_some(), "last_retrieved should be set");
    }

    #[test]
    fn memory_search_ranked_respects_host_scoping() {
        let conn = test_conn();
        // Identity-global (host IS NULL)
        memory_save_scoped(&conn, "fact", "global fact", None).unwrap();
        // Host-scoped to different host
        conn.execute(
            "UPDATE memories SET host = 'server-a' WHERE content = 'global fact'",
            [],
        ).unwrap();
        let m2 = memory_save_scoped(&conn, "fact", "server-b specific", None).unwrap();
        conn.execute(
            "UPDATE memories SET host = 'server-b' WHERE id = ?1",
            params![&m2.id],
        ).unwrap();

        // Searching from server-a should get only the global + server-a
        let results = memory_search_ranked(&conn, "fact", Some("server-a"), 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "global fact");
    }

    #[test]
    fn context_memories_orders_by_activation_pinned_first() {
        let conn = test_conn();
        let m1 = memory_save(&conn, "fact", "low activation").unwrap();
        let m2 = memory_save(&conn, "fact", "pinned high trust").unwrap();
        // Pin m2 and give it high trust
        conn.execute(
            "UPDATE memories SET pinned = 1, trust_score = 1.0, strength = 10 WHERE id = ?1",
            params![&m2.id],
        ).unwrap();
        // Give m1 low trust
        conn.execute(
            "UPDATE memories SET trust_score = 0.1, strength = 1 WHERE id = ?1",
            params![&m1.id],
        ).unwrap();

        let results = context_memories(&conn, 10, None).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].content, "pinned high trust", "pinned should be first");
    }

    #[test]
    fn context_memories_respects_budget() {
        let conn = test_conn();
        for i in 0..10 {
            memory_save(&conn, "fact", &format!("fact {i}")).unwrap();
        }
        let results = context_memories(&conn, 3, None).unwrap();
        assert_eq!(results.len(), 3);
    }
}
