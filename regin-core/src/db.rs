use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::{debug, info};

use crate::types::{
    Change, Conversation, Episode, Incident, Memory, Message, Problem, ProblemHypothesis, Schedule,
    TaskRun,
};

/// Initialize the SQLite database at the given path, creating tables if needed.
pub fn init_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open database: {}", path.display()))?;

    init_schema(&conn)?;

    info!("Database initialized at {}", path.display());
    Ok(conn)
}

/// Apply the full schema (idempotent) and seed default settings.
///
/// Split out from [`init_db`] so it can run against an in-memory connection in
/// tests.
pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id)
        );

        CREATE TABLE IF NOT EXISTS task_runs (
            id TEXT PRIMARY KEY,
            skill_name TEXT NOT NULL,
            status TEXT NOT NULL,
            output TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            category TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            repo_key TEXT,
            strength INTEGER NOT NULL DEFAULT 1,
            last_seen TEXT,
            source TEXT NOT NULL DEFAULT 'human'
        );

        CREATE TABLE IF NOT EXISTS repo_context (
            repo_key TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS repo_skills (
            repo_key TEXT NOT NULL,
            name TEXT NOT NULL,
            content TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (repo_key, name)
        );

        CREATE TABLE IF NOT EXISTS schedules (
            id TEXT PRIMARY KEY,
            skill_name TEXT NOT NULL UNIQUE,
            interval TEXT NOT NULL,
            next_run TEXT NOT NULL,
            last_run TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS incidents (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            severity TEXT NOT NULL,
            status TEXT NOT NULL,
            source TEXT NOT NULL,
            skill_name TEXT,
            workaround TEXT,
            opened_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            resolved_at TEXT,
            resolution TEXT
        );

        CREATE TABLE IF NOT EXISTS changes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            status TEXT NOT NULL,
            incident_id TEXT,
            problem_id TEXT,
            before_state TEXT,
            after_state TEXT,
            approved_by TEXT,
            approved_at TEXT,
            created_at TEXT NOT NULL,
            applied_at TEXT
        );

        CREATE TABLE IF NOT EXISTS problems (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            status TEXT NOT NULL,
            root_cause TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            closed_at TEXT
        );

        CREATE TABLE IF NOT EXISTS problem_incidents (
            problem_id TEXT NOT NULL,
            incident_id TEXT NOT NULL,
            PRIMARY KEY (problem_id, incident_id)
        );

        CREATE TABLE IF NOT EXISTS problem_hypotheses (
            id TEXT PRIMARY KEY,
            problem_id TEXT NOT NULL,
            text TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS episodes (
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            ref_id TEXT,
            summary TEXT NOT NULL,
            detail TEXT,
            created_at TEXT NOT NULL,
            reflected INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS kpi_events (
            id TEXT PRIMARY KEY,
            recorded_at TEXT NOT NULL,
            metric TEXT NOT NULL,
            value REAL NOT NULL,
            meta TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_kpi_events_metric_time
            ON kpi_events (metric, recorded_at);

        CREATE TABLE IF NOT EXISTS derived_checks (
            id TEXT PRIMARY KEY,
            domain TEXT NOT NULL,
            signature TEXT NOT NULL,
            description TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            demoted_at TEXT,
            demote_reason TEXT
        );

        CREATE TABLE IF NOT EXISTS objectives (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            metric TEXT NOT NULL,
            aggregate TEXT NOT NULL,
            window_days INTEGER NOT NULL,
            op TEXT NOT NULL,
            value_num REAL,
            value_text TEXT,
            priority INTEGER NOT NULL,
            source TEXT NOT NULL,
            rag TEXT NOT NULL DEFAULT 'green',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS goals (
            id TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            target TEXT NOT NULL,
            deadline TEXT NOT NULL,
            criteria_json TEXT NOT NULL,
            priority INTEGER NOT NULL,
            source TEXT NOT NULL,
            rag TEXT NOT NULL DEFAULT 'green',
            status TEXT NOT NULL DEFAULT 'proposed',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS intent_relations (
            id TEXT PRIMARY KEY,
            from_kind TEXT NOT NULL,
            from_id TEXT NOT NULL,
            to_kind TEXT NOT NULL,
            to_id TEXT NOT NULL,
            relation TEXT NOT NULL,
            credited_at TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_intent_relations_from
            ON intent_relations (from_kind, from_id, relation);
        CREATE INDEX IF NOT EXISTS idx_intent_relations_to
            ON intent_relations (to_kind, to_id, relation);

        CREATE TABLE IF NOT EXISTS intent_mitigations (
            id TEXT PRIMARY KEY,
            winner_kind TEXT NOT NULL,
            winner_id TEXT NOT NULL,
            deferred_kind TEXT NOT NULL,
            deferred_id TEXT NOT NULL,
            note TEXT NOT NULL,
            created_at TEXT NOT NULL
        );",
    )
    .context("Failed to create database tables")?;

    // Migrations: add memories columns to pre-existing databases (idempotent).
    add_column_if_missing(conn, "memories", "repo_key", "TEXT")?;
    add_column_if_missing(conn, "memories", "strength", "INTEGER NOT NULL DEFAULT 1")?;
    add_column_if_missing(conn, "memories", "last_seen", "TEXT")?;
    add_column_if_missing(conn, "memories", "source", "TEXT NOT NULL DEFAULT 'human'")?;

    // FEAT-035: ITIL schema extensions (idempotent on pre-existing databases).
    add_column_if_missing(conn, "incidents", "workaround", "TEXT")?;
    // The redundant incidents.problem_id is dropped; the problem_incidents join is
    // the single source of linkage (DISC-011). Any prior linkage was already
    // mirrored into that join by link_incident_to_problem.
    migrate_incident_problem_id_to_join(conn)?;
    drop_column_if_exists(conn, "incidents", "problem_id")?;
    add_column_if_missing(conn, "changes", "problem_id", "TEXT")?;
    add_column_if_missing(conn, "changes", "approved_by", "TEXT")?;
    add_column_if_missing(conn, "changes", "approved_at", "TEXT")?;

    // Seed defaults for any missing settings
    for (key, default, _desc) in crate::config::SETTINGS {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, default],
        )?;
    }

    Ok(())
}

/// Add `column` to `table` if it does not already exist (idempotent migration).
fn add_column_if_missing(conn: &Connection, table: &str, column: &str, decl: &str) -> Result<()> {
    let exists: bool = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
            params![column],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);
    if !exists {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"), [])?;
    }
    Ok(())
}

/// Drop `column` from `table` if it still exists (idempotent migration). Used to
/// retire the redundant `incidents.problem_id` (DISC-011/FEAT-035).
fn drop_column_if_exists(conn: &Connection, table: &str, column: &str) -> Result<()> {
    let exists: bool = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
            params![column],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);
    if exists {
        conn.execute(&format!("ALTER TABLE {table} DROP COLUMN {column}"), [])?;
    }
    Ok(())
}

/// Before dropping `incidents.problem_id`, fold any linkage it still holds into the
/// `problem_incidents` join so no data is lost (FEAT-035). No-op once the column is
/// gone.
fn migrate_incident_problem_id_to_join(conn: &Connection) -> Result<()> {
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('incidents') WHERE name = 'problem_id'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);
    if has_col {
        conn.execute(
            "INSERT OR IGNORE INTO problem_incidents (problem_id, incident_id)
             SELECT problem_id, id FROM incidents WHERE problem_id IS NOT NULL",
            [],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Get a setting value, returning the default if not set.
pub fn setting_get(conn: &Connection, key: &str) -> Result<String> {
    let result: std::result::Result<String, _> = conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    );
    match result {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            // Return compiled default if known
            for (k, default, _) in crate::config::SETTINGS {
                if *k == key {
                    return Ok(default.to_string());
                }
            }
            Ok(String::new())
        }
        Err(e) => Err(e.into()),
    }
}

/// Set a setting value.
pub fn setting_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    debug!(key, value, "Setting updated");
    Ok(())
}

/// List all settings as (key, value) pairs.
pub fn setting_list(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .context("Failed to query settings")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read settings rows")?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Messages & Conversations
// ---------------------------------------------------------------------------

pub fn save_message(
    conn: &Connection,
    conversation_id: &str,
    conversation_title: &str,
    role: &str,
    content: &str,
) -> Result<Message> {
    let now = chrono::Utc::now().to_rfc3339();
    let msg_id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO conversations (id, title, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET updated_at = ?4",
        params![conversation_id, conversation_title, &now, &now],
    )
    .context("Failed to upsert conversation")?;

    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&msg_id, conversation_id, role, content, &now],
    )
    .context("Failed to insert message")?;

    debug!(conversation_id, role, "Message saved");
    Ok(Message {
        id: msg_id,
        conversation_id: conversation_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        created_at: now,
    })
}

pub fn get_conversation_messages(conn: &Connection, conversation_id: &str) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, created_at
         FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
    )?;
    let messages = stmt
        .query_map(params![conversation_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(messages)
}

pub fn list_conversations(conn: &Connection) -> Result<Vec<Conversation>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, updated_at
         FROM conversations ORDER BY updated_at DESC",
    )?;
    let convos = stmt
        .query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(convos)
}

// ---------------------------------------------------------------------------
// Task Runs
// ---------------------------------------------------------------------------

pub fn save_task_run(
    conn: &Connection,
    skill_name: &str,
    status: &str,
    output: &str,
    started_at: &str,
    finished_at: &str,
) -> Result<TaskRun> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO task_runs (id, skill_name, status, output, started_at, finished_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![&id, skill_name, status, output, started_at, finished_at],
    )?;
    debug!(skill_name, status, "Task run saved");
    Ok(TaskRun {
        id,
        skill_name: skill_name.to_string(),
        status: status.to_string(),
        output: output.to_string(),
        started_at: started_at.to_string(),
        finished_at: finished_at.to_string(),
    })
}

pub fn get_task_runs(conn: &Connection, skill_name: &str, limit: usize) -> Result<Vec<TaskRun>> {
    let mut stmt = conn.prepare(
        "SELECT id, skill_name, status, output, started_at, finished_at
         FROM task_runs WHERE skill_name = ?1 ORDER BY started_at DESC LIMIT ?2",
    )?;
    let runs = stmt
        .query_map(params![skill_name, limit as i64], |row| {
            Ok(TaskRun {
                id: row.get(0)?,
                skill_name: row.get(1)?,
                status: row.get(2)?,
                output: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(runs)
}

pub fn get_all_task_runs(conn: &Connection, limit: usize) -> Result<Vec<TaskRun>> {
    let mut stmt = conn.prepare(
        "SELECT id, skill_name, status, output, started_at, finished_at
         FROM task_runs ORDER BY started_at DESC LIMIT ?1",
    )?;
    let runs = stmt
        .query_map(params![limit as i64], |row| {
            Ok(TaskRun {
                id: row.get(0)?,
                skill_name: row.get(1)?,
                status: row.get(2)?,
                output: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(runs)
}

// ---------------------------------------------------------------------------
// Memories
// ---------------------------------------------------------------------------

const MEMORY_COLS: &str =
    "id, category, content, created_at, updated_at, strength, last_seen, source";

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

pub fn memory_save(conn: &Connection, category: &str, content: &str) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO memories (id, category, content, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, category, content, &now, &now],
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

/// Save a memory scoped to a repo (`repo_key = Some`) or global (`None`).
pub fn memory_save_scoped(
    conn: &Connection,
    category: &str,
    content: &str,
    repo_key: Option<&str>,
) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO memories (id, category, content, created_at, updated_at, repo_key)
         VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
        params![&id, category, content, &now, repo_key],
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

/// Insert a memory distilled by reflection (source = reflection, FEAT-006).
pub fn memory_save_reflection(conn: &Connection, category: &str, content: &str) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO memories (id, category, content, created_at, updated_at, strength, last_seen, source)
         VALUES (?1, ?2, ?3, ?4, ?4, 1, ?4, 'reflection')",
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

/// Reinforce a memory: strength += 1, last_seen = now.
pub fn memory_reinforce(conn: &Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE memories SET strength = strength + 1, last_seen = ?1, updated_at = ?1 WHERE id = ?2",
        params![&now, id],
    )?;
    Ok(())
}

/// Find an existing memory with the same category and (trimmed, case-insensitive)
/// content — the merge target for a reflection proposal. Returns its id.
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

/// Decay reflection memories not seen since `before`: strength -= 1, then drop
/// any that reach 0. `human` memories are never touched. Returns the number
/// dropped.
pub fn memory_decay(conn: &Connection, before: &str) -> Result<usize> {
    conn.execute(
        "UPDATE memories SET strength = strength - 1
         WHERE source = 'reflection' AND strength > 0
           AND (last_seen IS NULL OR last_seen < ?1)",
        params![before],
    )?;
    let dropped = conn.execute(
        "DELETE FROM memories WHERE source = 'reflection' AND strength <= 0",
        [],
    )?;
    Ok(dropped)
}

/// Memories applicable to a repo: globals (`repo_key IS NULL`) plus the repo's
/// own. With `None`, only globals. Used for context injection (FEAT-008) so a
/// repo's memories never leak into another.
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

/// Get the stored per-repo context for a repo key, if any.
pub fn repo_context_get(conn: &Connection, repo_key: &str) -> Result<Option<String>> {
    let r: std::result::Result<String, _> = conn.query_row(
        "SELECT content FROM repo_context WHERE repo_key = ?1",
        params![repo_key],
        |row| row.get(0),
    );
    match r {
        Ok(c) => Ok(Some(c)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Set (upsert) the per-repo context for a repo key.
pub fn repo_context_set(conn: &Connection, repo_key: &str, content: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO repo_context (repo_key, content, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(repo_key) DO UPDATE SET content = excluded.content, updated_at = excluded.updated_at",
        params![repo_key, content, &now],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-repo skills (FEAT-009)
// ---------------------------------------------------------------------------

/// Save (upsert) a per-repo skill keyed by repo path.
pub fn repo_skill_save(conn: &Connection, repo_key: &str, name: &str, content: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO repo_skills (repo_key, name, content, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(repo_key, name) DO UPDATE SET content = excluded.content, updated_at = excluded.updated_at",
        params![repo_key, name, content, &now],
    )?;
    Ok(())
}

/// List a repo's per-repo skills as (name, content), name-sorted.
pub fn repo_skill_list(conn: &Connection, repo_key: &str) -> Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT name, content FROM repo_skills WHERE repo_key = ?1 ORDER BY name")?;
    let rows = stmt
        .query_map(params![repo_key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Get a single per-repo skill's content.
pub fn repo_skill_get(conn: &Connection, repo_key: &str, name: &str) -> Result<Option<String>> {
    let r: std::result::Result<String, _> = conn.query_row(
        "SELECT content FROM repo_skills WHERE repo_key = ?1 AND name = ?2",
        params![repo_key, name],
        |row| row.get(0),
    );
    match r {
        Ok(c) => Ok(Some(c)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete a per-repo skill.
pub fn repo_skill_delete(conn: &Connection, repo_key: &str, name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM repo_skills WHERE repo_key = ?1 AND name = ?2",
        params![repo_key, name],
    )?;
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

// ---------------------------------------------------------------------------
// Schedules
// ---------------------------------------------------------------------------

pub fn save_schedule(
    conn: &Connection,
    skill_name: &str,
    interval: &str,
    next_run: &str,
) -> Result<Schedule> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO schedules (id, skill_name, interval, next_run, last_run, created_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5)
         ON CONFLICT(skill_name) DO UPDATE SET interval = excluded.interval, next_run = excluded.next_run",
        params![&id, skill_name, interval, next_run, &now],
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, skill_name, interval, next_run, last_run FROM schedules WHERE skill_name = ?1",
    )?;
    let schedule = stmt.query_row(params![skill_name], |row| {
        Ok(Schedule {
            id: row.get(0)?,
            skill: row.get(1)?,
            interval: row.get(2)?,
            next_run: row.get(3)?,
            last_run: row.get(4)?,
        })
    })?;
    Ok(schedule)
}

pub fn delete_schedule(conn: &Connection, skill_name: &str) -> Result<()> {
    conn.execute("DELETE FROM schedules WHERE skill_name = ?1", params![skill_name])?;
    Ok(())
}

pub fn list_schedules(conn: &Connection) -> Result<Vec<Schedule>> {
    let mut stmt = conn.prepare(
        "SELECT id, skill_name, interval, next_run, last_run FROM schedules ORDER BY next_run ASC",
    )?;
    let schedules = stmt
        .query_map([], |row| {
            Ok(Schedule {
                id: row.get(0)?,
                skill: row.get(1)?,
                interval: row.get(2)?,
                next_run: row.get(3)?,
                last_run: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(schedules)
}

pub fn get_due_schedules(conn: &Connection, now: &str) -> Result<Vec<Schedule>> {
    let mut stmt = conn.prepare(
        "SELECT id, skill_name, interval, next_run, last_run
         FROM schedules WHERE next_run <= ?1 ORDER BY next_run ASC",
    )?;
    let schedules = stmt
        .query_map(params![now], |row| {
            Ok(Schedule {
                id: row.get(0)?,
                skill: row.get(1)?,
                interval: row.get(2)?,
                next_run: row.get(3)?,
                last_run: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(schedules)
}

pub fn update_schedule_after_run(
    conn: &Connection,
    skill_name: &str,
    last_run: &str,
    next_run: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE schedules SET last_run = ?1, next_run = ?2 WHERE skill_name = ?3",
        params![last_run, next_run, skill_name],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ITIL: Incidents (FEAT-002)
// ---------------------------------------------------------------------------

const INCIDENT_COLS: &str =
    "id, title, description, severity, status, source, skill_name, workaround, \
     opened_at, updated_at, resolved_at, resolution";

fn row_to_incident(row: &rusqlite::Row) -> rusqlite::Result<Incident> {
    Ok(Incident {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        severity: row.get(3)?,
        status: row.get(4)?,
        source: row.get(5)?,
        skill_name: row.get(6)?,
        workaround: row.get(7)?,
        opened_at: row.get(8)?,
        updated_at: row.get(9)?,
        resolved_at: row.get(10)?,
        resolution: row.get(11)?,
    })
}

/// Open a new incident (status = open).
pub fn incident_open(
    conn: &Connection,
    title: &str,
    description: &str,
    severity: &str,
    source: &str,
    skill_name: Option<&str>,
) -> Result<Incident> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO incidents
            (id, title, description, severity, status, source, skill_name, workaround,
             opened_at, updated_at, resolved_at, resolution)
         VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6, NULL, ?7, ?7, NULL, NULL)",
        params![&id, title, description, severity, source, skill_name, &now],
    )?;
    debug!(id, title, "Incident opened");
    incident_get(conn, &id)?.context("incident vanished after insert")
}

/// Block an incident on a workaround (status = blocked) while its underlying
/// problem awaits a real fix (FEAT-035).
pub fn incident_block(conn: &Connection, id: &str, workaround: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE incidents SET status = 'blocked', workaround = ?1, updated_at = ?2 WHERE id = ?3",
        params![workaround, &now, id],
    )?;
    Ok(())
}

pub fn incident_get(conn: &Connection, id: &str) -> Result<Option<Incident>> {
    let sql = format!("SELECT {INCIDENT_COLS} FROM incidents WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_incident)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn incident_list(conn: &Connection, status: Option<&str>) -> Result<Vec<Incident>> {
    let (sql, p): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
        Some(s) => (
            format!("SELECT {INCIDENT_COLS} FROM incidents WHERE status = ?1 ORDER BY opened_at DESC"),
            vec![Box::new(s.to_string())],
        ),
        None => (
            format!("SELECT {INCIDENT_COLS} FROM incidents ORDER BY opened_at DESC"),
            vec![],
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), row_to_incident)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Set an incident's status (e.g. open -> investigating); bumps updated_at.
pub fn incident_set_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE incidents SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, &now, id],
    )?;
    Ok(())
}

/// Resolve an incident: status = resolved, record the resolution + timestamp.
pub fn incident_resolve(conn: &Connection, id: &str, resolution: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE incidents
         SET status = 'resolved', resolution = ?1, resolved_at = ?2, updated_at = ?2
         WHERE id = ?3",
        params![resolution, &now, id],
    )?;
    Ok(())
}

/// Close an incident (status = closed).
pub fn incident_close(conn: &Connection, id: &str) -> Result<()> {
    incident_set_status(conn, id, "closed")
}

// ---------------------------------------------------------------------------
// ITIL: Changes (FEAT-002)
// ---------------------------------------------------------------------------

const CHANGE_COLS: &str =
    "id, title, description, status, incident_id, problem_id, before_state, after_state, \
     approved_by, approved_at, created_at, applied_at";

fn row_to_change(row: &rusqlite::Row) -> rusqlite::Result<Change> {
    Ok(Change {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        incident_id: row.get(4)?,
        problem_id: row.get(5)?,
        before: row.get(6)?,
        after: row.get(7)?,
        approved_by: row.get(8)?,
        approved_at: row.get(9)?,
        created_at: row.get(10)?,
        applied_at: row.get(11)?,
    })
}

/// Record a change (status = planned). `incident_id` and `problem_id` link the
/// change to what it remediates / resolves (either or both may be `None`).
pub fn change_record(
    conn: &Connection,
    title: &str,
    description: &str,
    incident_id: Option<&str>,
    problem_id: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<Change> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO changes
            (id, title, description, status, incident_id, problem_id, before_state, after_state,
             approved_by, approved_at, created_at, applied_at)
         VALUES (?1, ?2, ?3, 'planned', ?4, ?5, ?6, ?7, NULL, NULL, ?8, NULL)",
        params![&id, title, description, incident_id, problem_id, before, after, &now],
    )?;
    change_get(conn, &id)?.context("change vanished after insert")
}

/// Move a change to `pending_approval` — staged but awaiting a human/supervisor
/// decision before it may be applied (FEAT-035).
pub fn change_request_approval(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE changes SET status = 'pending_approval' WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Approve a `pending_approval` change, recording the approver and time. The
/// change returns to `planned` so the normal apply path can run; `approved_at`
/// stamps the decision (FEAT-035).
pub fn change_approve(conn: &Connection, id: &str, approved_by: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE changes SET status = 'planned', approved_by = ?1, approved_at = ?2 WHERE id = ?3",
        params![approved_by, &now, id],
    )?;
    Ok(())
}

pub fn change_get(conn: &Connection, id: &str) -> Result<Option<Change>> {
    let sql = format!("SELECT {CHANGE_COLS} FROM changes WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_change)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn change_list(conn: &Connection) -> Result<Vec<Change>> {
    let sql = format!("SELECT {CHANGE_COLS} FROM changes ORDER BY created_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_change)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Mark a change applied (status = applied, applied_at = now).
pub fn change_apply(conn: &Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE changes SET status = 'applied', applied_at = ?1 WHERE id = ?2",
        params![&now, id],
    )?;
    Ok(())
}

/// Close a change (status = closed).
pub fn change_close(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE changes SET status = 'closed' WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ITIL: Problems (FEAT-002)
// ---------------------------------------------------------------------------

const PROBLEM_COLS: &str =
    "id, title, description, status, root_cause, created_at, updated_at, closed_at";

fn row_to_problem(row: &rusqlite::Row) -> rusqlite::Result<Problem> {
    Ok(Problem {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        root_cause: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        closed_at: row.get(7)?,
    })
}

/// Open a new problem (status = open).
pub fn problem_open(conn: &Connection, title: &str, description: &str) -> Result<Problem> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO problems
            (id, title, description, status, root_cause, created_at, updated_at, closed_at)
         VALUES (?1, ?2, ?3, 'open', NULL, ?4, ?4, NULL)",
        params![&id, title, description, &now],
    )?;
    problem_get(conn, &id)?.context("problem vanished after insert")
}

pub fn problem_get(conn: &Connection, id: &str) -> Result<Option<Problem>> {
    let sql = format!("SELECT {PROBLEM_COLS} FROM problems WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_problem)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn problem_list(conn: &Connection, status: Option<&str>) -> Result<Vec<Problem>> {
    let (sql, p): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
        Some(s) => (
            format!("SELECT {PROBLEM_COLS} FROM problems WHERE status = ?1 ORDER BY created_at DESC"),
            vec![Box::new(s.to_string())],
        ),
        None => (
            format!("SELECT {PROBLEM_COLS} FROM problems ORDER BY created_at DESC"),
            vec![],
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), row_to_problem)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Promote a problem to a known error with a recorded root cause.
pub fn problem_set_known_error(conn: &Connection, id: &str, root_cause: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE problems SET status = 'known_error', root_cause = ?1, updated_at = ?2 WHERE id = ?3",
        params![root_cause, &now, id],
    )?;
    Ok(())
}

/// Close a problem (status = closed, closed_at = now).
pub fn problem_close(conn: &Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE problems SET status = 'closed', closed_at = ?1, updated_at = ?1 WHERE id = ?2",
        params![&now, id],
    )?;
    Ok(())
}

/// Link an incident to a problem (idempotent). Linkage lives solely in the
/// `problem_incidents` join (the `incidents.problem_id` column was retired in
/// FEAT-035); the incident's `updated_at` is bumped to reflect the change.
pub fn link_incident_to_problem(conn: &Connection, problem_id: &str, incident_id: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO problem_incidents (problem_id, incident_id) VALUES (?1, ?2)",
        params![problem_id, incident_id],
    )?;
    incident_touch(conn, incident_id)?;
    Ok(())
}

/// The incident ids linked to a problem.
pub fn problem_incident_ids(conn: &Connection, problem_id: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT incident_id FROM problem_incidents WHERE problem_id = ?1 ORDER BY incident_id")?;
    let rows = stmt
        .query_map(params![problem_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The problem an incident is linked to, if any (via the join). Replaces the old
/// `incidents.problem_id` column read (FEAT-035).
pub fn incident_problem_id(conn: &Connection, incident_id: &str) -> Result<Option<String>> {
    let r: std::result::Result<String, _> = conn.query_row(
        "SELECT problem_id FROM problem_incidents WHERE incident_id = ?1 LIMIT 1",
        params![incident_id],
        |row| row.get(0),
    );
    match r {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ---------------------------------------------------------------------------
// ITIL: Problem hypotheses (FEAT-035)
// ---------------------------------------------------------------------------

const HYPOTHESIS_COLS: &str = "id, problem_id, text, status, created_at, updated_at";

fn row_to_hypothesis(row: &rusqlite::Row) -> rusqlite::Result<ProblemHypothesis> {
    Ok(ProblemHypothesis {
        id: row.get(0)?,
        problem_id: row.get(1)?,
        text: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

/// Add a hypothesis to a problem (status = created).
pub fn hypothesis_add(conn: &Connection, problem_id: &str, text: &str) -> Result<ProblemHypothesis> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO problem_hypotheses (id, problem_id, text, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'created', ?4, ?4)",
        params![&id, problem_id, text, &now],
    )?;
    let sql = format!("SELECT {HYPOTHESIS_COLS} FROM problem_hypotheses WHERE id = ?1");
    conn.query_row(&sql, params![&id], row_to_hypothesis)
        .context("hypothesis vanished after insert")
}

/// List a problem's hypotheses, oldest first.
pub fn hypothesis_list(conn: &Connection, problem_id: &str) -> Result<Vec<ProblemHypothesis>> {
    let sql = format!(
        "SELECT {HYPOTHESIS_COLS} FROM problem_hypotheses WHERE problem_id = ?1 ORDER BY created_at ASC, rowid ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![problem_id], row_to_hypothesis)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Set a hypothesis's status (created | validating | confirmed | rejected).
pub fn hypothesis_set_status(conn: &Connection, id: &str, status: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE problem_hypotheses SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, &now, id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Episodic memory (FEAT-005)
// ---------------------------------------------------------------------------

fn row_to_episode(row: &rusqlite::Row) -> rusqlite::Result<Episode> {
    Ok(Episode {
        id: row.get(0)?,
        kind: row.get(1)?,
        ref_id: row.get(2)?,
        summary: row.get(3)?,
        detail: row.get(4)?,
        created_at: row.get(5)?,
        reflected: row.get::<_, i64>(6)? != 0,
    })
}

/// Record an episode (reflected = false).
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
        "INSERT INTO episodes (id, kind, ref_id, summary, detail, created_at, reflected)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
        params![&id, kind, ref_id, summary, detail, &now],
    )?;
    debug!(id, kind, "Episode recorded");
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

/// The most recent *unreflected* episodes, newest first, bounded by `limit`.
pub fn episode_recent(conn: &Connection, limit: usize) -> Result<Vec<Episode>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, ref_id, summary, detail, created_at, reflected
         FROM episodes WHERE reflected = 0 ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], row_to_episode)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Mark the given episodes reflected so the next reflection pass skips them.
pub fn episode_mark_reflected(conn: &Connection, ids: &[String]) -> Result<()> {
    for id in ids {
        conn.execute("UPDATE episodes SET reflected = 1 WHERE id = ?1", params![id])?;
    }
    Ok(())
}

/// Prune *reflected* episodes created before `before` (RFC3339). Unreflected
/// episodes are never removed. Returns the number deleted.
pub fn episode_prune(conn: &Connection, before: &str) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM episodes WHERE reflected = 1 AND created_at < ?1",
        params![before],
    )?;
    Ok(n)
}

// ---------------------------------------------------------------------------
// Monitoring evaluation -> incidents/problems (FEAT-004)
// ---------------------------------------------------------------------------

/// The most recent *active* (open|investigating) incident for a skill, if any.
pub fn incident_active_for_skill(conn: &Connection, skill_name: &str) -> Result<Option<Incident>> {
    let sql = format!(
        "SELECT {INCIDENT_COLS} FROM incidents
         WHERE skill_name = ?1 AND status IN ('open','investigating')
         ORDER BY opened_at DESC LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![skill_name], row_to_incident)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// All incidents for a skill, across all statuses (newest first).
pub fn incidents_for_skill(conn: &Connection, skill_name: &str) -> Result<Vec<Incident>> {
    let sql = format!("SELECT {INCIDENT_COLS} FROM incidents WHERE skill_name = ?1 ORDER BY opened_at DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![skill_name], row_to_incident)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Bump an incident's updated_at without changing its status.
pub fn incident_touch(conn: &Connection, id: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute("UPDATE incidents SET updated_at = ?1 WHERE id = ?2", params![&now, id])?;
    Ok(())
}

/// Outcome of evaluating a monitoring (scheduled task) result.
#[derive(Debug, Clone, Default)]
pub struct MonitorOutcome {
    /// The incident opened or updated, if the run failed.
    pub incident_id: Option<String>,
    /// Whether a *new* incident was created (vs. an existing one updated).
    pub created_incident: bool,
    /// The problem opened/linked when the recurrence threshold was reached.
    pub problem_id: Option<String>,
}

/// Evaluate a scheduled run's result. Deterministic first pass:
/// - `success` is a no-op.
/// - a non-success run opens an incident for the skill, *unless* an active
///   incident already exists for it (then that incident is updated — no
///   duplicate).
/// - when the number of incidents for the skill reaches `recurrence_threshold`,
///   a problem is opened (or the existing one reused) and the incidents linked.
///
/// The signature is the skill name (one "shape" per skill); this can be refined
/// later with an error fingerprint.
pub fn monitor_evaluate(
    conn: &Connection,
    skill_name: &str,
    status: &str,
    output: &str,
    severity: &str,
    recurrence_threshold: usize,
) -> Result<MonitorOutcome> {
    if status == "success" {
        return Ok(MonitorOutcome::default());
    }

    let (incident_id, created_incident) = match incident_active_for_skill(conn, skill_name)? {
        Some(existing) => {
            incident_touch(conn, &existing.id)?;
            (existing.id, false)
        }
        None => {
            let preview: String = output.chars().take(200).collect();
            let inc = incident_open(
                conn,
                &format!("{skill_name} failed"),
                &preview,
                severity,
                "monitor",
                Some(skill_name),
            )?;
            episode_record(
                conn,
                "incident",
                Some(&inc.id),
                &format!("monitor opened incident for `{skill_name}`"),
                Some(&preview),
            )?;
            (inc.id, true)
        }
    };

    let all = incidents_for_skill(conn, skill_name)?;
    let problem_id = if recurrence_threshold > 0 && all.len() >= recurrence_threshold {
        // Reuse an existing problem already linked to any of this skill's incidents.
        let mut existing_pid = None;
        for i in &all {
            if let Some(p) = incident_problem_id(conn, &i.id)? {
                existing_pid = Some(p);
                break;
            }
        }
        let pid = match existing_pid {
            Some(existing) => existing,
            None => {
                problem_open(
                    conn,
                    &format!("recurring failures: {skill_name}"),
                    &format!("{} incidents recorded for `{skill_name}`", all.len()),
                )?
                .id
            }
        };
        for i in &all {
            link_incident_to_problem(conn, &pid, &i.id)?;
        }
        Some(pid)
    } else {
        None
    };

    Ok(MonitorOutcome { incident_id: Some(incident_id), created_incident, problem_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn init_schema_is_idempotent() {
        let conn = test_conn();
        // Running it again must not error.
        init_schema(&conn).unwrap();
    }

    #[test]
    fn incident_lifecycle() {
        let conn = test_conn();
        let inc = incident_open(&conn, "disk full on /var", "log spike", "high", "monitor", Some("disk-usage")).unwrap();
        assert_eq!(inc.status, "open");
        assert_eq!(inc.severity, "high");
        assert_eq!(inc.source, "monitor");
        assert_eq!(inc.skill_name.as_deref(), Some("disk-usage"));

        incident_set_status(&conn, &inc.id, "investigating").unwrap();
        assert_eq!(incident_get(&conn, &inc.id).unwrap().unwrap().status, "investigating");
        assert_eq!(incident_list(&conn, Some("investigating")).unwrap().len(), 1);
        assert_eq!(incident_list(&conn, Some("open")).unwrap().len(), 0);

        incident_resolve(&conn, &inc.id, "rotated logs").unwrap();
        let r = incident_get(&conn, &inc.id).unwrap().unwrap();
        assert_eq!(r.status, "resolved");
        assert_eq!(r.resolution.as_deref(), Some("rotated logs"));
        assert!(r.resolved_at.is_some());

        incident_close(&conn, &inc.id).unwrap();
        assert_eq!(incident_get(&conn, &inc.id).unwrap().unwrap().status, "closed");
    }

    #[test]
    fn change_lifecycle() {
        let conn = test_conn();
        let inc = incident_open(&conn, "svc down", "", "medium", "manual", None).unwrap();
        let chg = change_record(
            &conn,
            "restart nginx",
            "remediate the outage",
            Some(&inc.id),
            None,
            Some("stopped"),
            Some("running"),
        )
        .unwrap();
        assert_eq!(chg.status, "planned");
        assert_eq!(chg.incident_id.as_deref(), Some(inc.id.as_str()));
        assert_eq!(chg.before.as_deref(), Some("stopped"));

        change_apply(&conn, &chg.id).unwrap();
        let applied = change_get(&conn, &chg.id).unwrap().unwrap();
        assert_eq!(applied.status, "applied");
        assert!(applied.applied_at.is_some());

        change_close(&conn, &chg.id).unwrap();
        assert_eq!(change_get(&conn, &chg.id).unwrap().unwrap().status, "closed");
        assert_eq!(change_list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn problem_lifecycle_and_linking() {
        let conn = test_conn();
        let prob = problem_open(&conn, "nightly job flaps", "recurring timeout").unwrap();
        assert_eq!(prob.status, "open");

        let i1 = incident_open(&conn, "timeout #1", "", "low", "monitor", Some("nightly")).unwrap();
        let i2 = incident_open(&conn, "timeout #2", "", "low", "monitor", Some("nightly")).unwrap();
        link_incident_to_problem(&conn, &prob.id, &i1.id).unwrap();
        link_incident_to_problem(&conn, &prob.id, &i2.id).unwrap();
        // idempotent
        link_incident_to_problem(&conn, &prob.id, &i1.id).unwrap();

        let mut linked = problem_incident_ids(&conn, &prob.id).unwrap();
        linked.sort();
        let mut expected = vec![i1.id.clone(), i2.id.clone()];
        expected.sort();
        assert_eq!(linked, expected);
        assert_eq!(incident_problem_id(&conn, &i1.id).unwrap().as_deref(), Some(prob.id.as_str()));

        problem_set_known_error(&conn, &prob.id, "deadlock in cron lock").unwrap();
        let ke = problem_get(&conn, &prob.id).unwrap().unwrap();
        assert_eq!(ke.status, "known_error");
        assert_eq!(ke.root_cause.as_deref(), Some("deadlock in cron lock"));

        problem_close(&conn, &prob.id).unwrap();
        let closed = problem_get(&conn, &prob.id).unwrap().unwrap();
        assert_eq!(closed.status, "closed");
        assert!(closed.closed_at.is_some());
    }

    #[test]
    fn incident_get_missing_returns_none() {
        let conn = test_conn();
        assert!(incident_get(&conn, "nope").unwrap().is_none());
    }

    fn episode_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn episode_record_recent_and_reflect() {
        let conn = test_conn();
        let e1 = episode_record(&conn, "task_run", Some("run-1"), "ran disk-usage", None).unwrap();
        let _e2 = episode_record(&conn, "incident", Some("inc-1"), "opened incident", None).unwrap();
        let e3 = episode_record(&conn, "chat", None, "chatted", Some("detail")).unwrap();
        assert!(!e1.reflected);

        // newest first, all unreflected
        let recent = episode_recent(&conn, 10).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent.first().unwrap().id, e3.id, "newest first");
        assert_eq!(recent.last().unwrap().id, e1.id, "oldest last");

        // limit is honoured
        assert_eq!(episode_recent(&conn, 2).unwrap().len(), 2);

        // reflected episodes drop out of `recent`
        episode_mark_reflected(&conn, &[recent[1].id.clone()]).unwrap();
        let after = episode_recent(&conn, 10).unwrap();
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|e| e.id != recent[1].id));
    }

    #[test]
    fn episode_prune_removes_only_old_reflected() {
        let conn = test_conn();
        let a = episode_record(&conn, "task_run", None, "a", None).unwrap();
        let _b = episode_record(&conn, "task_run", None, "b", None).unwrap();
        // mark `a` reflected; leave `b` unreflected
        episode_mark_reflected(&conn, &[a.id.clone()]).unwrap();

        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let deleted = episode_prune(&conn, &future).unwrap();
        assert_eq!(deleted, 1, "only the reflected episode is pruned");
        assert_eq!(episode_count(&conn), 1, "the unreflected episode survives");

        // pruning with an old cutoff deletes nothing further
        let past = (chrono::Utc::now() - chrono::Duration::days(365)).to_rfc3339();
        assert_eq!(episode_prune(&conn, &past).unwrap(), 0);
    }

    #[test]
    fn per_repo_memories_do_not_leak() {
        let conn = test_conn();
        memory_save_scoped(&conn, "fact", "global fact", None).unwrap();
        memory_save_scoped(&conn, "fact", "repo A fact", Some("/repos/a")).unwrap();
        memory_save_scoped(&conn, "fact", "repo B fact", Some("/repos/b")).unwrap();

        let a: Vec<_> = memory_list_for_repo(&conn, Some("/repos/a")).unwrap().into_iter().map(|m| m.content).collect();
        assert!(a.contains(&"global fact".to_string()));
        assert!(a.contains(&"repo A fact".to_string()));
        assert!(!a.contains(&"repo B fact".to_string()), "repo B must not leak into repo A");

        let g: Vec<_> = memory_list_for_repo(&conn, None).unwrap().into_iter().map(|m| m.content).collect();
        assert_eq!(g, vec!["global fact".to_string()], "global scope sees only globals");
    }

    #[test]
    fn reflection_reinforce_decay_and_human_protection() {
        let conn = test_conn();
        // human memory: never decayed
        let h = memory_save(&conn, "preference", "always use apt").unwrap();
        // reflection memory
        let r = memory_save_reflection(&conn, "pattern", "/var/log fills weekly").unwrap();
        assert_eq!(r.source, "reflection");
        assert_eq!(r.strength, 1);

        // similar lookup matches case/space-insensitively
        let found = memory_find_similar(&conn, "pattern", "  /VAR/log fills weekly ").unwrap();
        assert_eq!(found.as_deref(), Some(r.id.as_str()));
        assert!(memory_find_similar(&conn, "fact", "nope").unwrap().is_none());

        // reinforce raises strength
        memory_reinforce(&conn, &r.id).unwrap();
        let got = memory_list(&conn, None).unwrap();
        let rr = got.iter().find(|m| m.id == r.id).unwrap();
        assert_eq!(rr.strength, 2);

        // decay: cutoff in the future -> reflection loses 1 (2 -> 1), survives;
        // human untouched.
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        assert_eq!(memory_decay(&conn, &future).unwrap(), 0);
        let after = memory_list(&conn, None).unwrap();
        assert_eq!(after.iter().find(|m| m.id == r.id).unwrap().strength, 1);
        assert_eq!(after.iter().find(|m| m.id == h.id).unwrap().strength, 1, "human strength unchanged");

        // decay again -> reflection hits 0 and is dropped; human remains
        assert_eq!(memory_decay(&conn, &future).unwrap(), 1);
        let end = memory_list(&conn, None).unwrap();
        assert!(end.iter().all(|m| m.id != r.id), "reflection memory dropped at strength 0");
        assert!(end.iter().any(|m| m.id == h.id), "human memory protected");
    }

    #[test]
    fn apply_reflection_reinforces_or_creates() {
        let conn = Connection::open_in_memory().unwrap();
        crate::identity_db::init_identity_schema(&conn).unwrap();
        crate::identity_db::memory_save_reflection(&conn, "pattern", "disk pressure on db01").unwrap();
        let proposals = vec![
            crate::reflect::ReflectionProposal { category: "pattern".into(), content: "disk pressure on db01".into() }, // matches -> reinforce
            crate::reflect::ReflectionProposal { category: "fact".into(), content: "db01 runs postgres 16".into() },    // new -> create
        ];
        let stats = crate::reflect::apply_reflection(&conn, &proposals).unwrap();
        assert_eq!(stats.reinforced, 1);
        assert_eq!(stats.created, 1);
        // the matched one now has strength 2, and a new fact exists
        let mems = crate::identity_db::memory_list(&conn, None).unwrap();
        assert_eq!(mems.iter().find(|m| m.content == "disk pressure on db01").unwrap().strength, 2);
        assert!(mems.iter().any(|m| m.category == "fact" && m.content == "db01 runs postgres 16"));
    }

    #[test]
    fn repo_context_set_get_upsert() {
        let conn = test_conn();
        assert!(repo_context_get(&conn, "/repos/a").unwrap().is_none());
        repo_context_set(&conn, "/repos/a", "first").unwrap();
        assert_eq!(repo_context_get(&conn, "/repos/a").unwrap().as_deref(), Some("first"));
        repo_context_set(&conn, "/repos/a", "second").unwrap();
        assert_eq!(repo_context_get(&conn, "/repos/a").unwrap().as_deref(), Some("second"));
        assert!(repo_context_get(&conn, "/repos/other").unwrap().is_none());
    }

    #[test]
    fn monitor_success_is_noop() {
        let conn = test_conn();
        let out = monitor_evaluate(&conn, "disk-usage", "success", "all good", "medium", 3).unwrap();
        assert!(out.incident_id.is_none());
        assert!(!out.created_incident);
        assert_eq!(incidents_for_skill(&conn, "disk-usage").unwrap().len(), 0);
    }

    #[test]
    fn monitor_dedups_while_incident_open() {
        let conn = test_conn();
        let first = monitor_evaluate(&conn, "nightly", "error", "boom", "high", 3).unwrap();
        assert!(first.created_incident);
        assert!(first.problem_id.is_none());

        // second failure while the incident is still open -> same incident, no dup
        let second = monitor_evaluate(&conn, "nightly", "error", "boom again", "high", 3).unwrap();
        assert!(!second.created_incident);
        assert_eq!(second.incident_id, first.incident_id);
        assert_eq!(incidents_for_skill(&conn, "nightly").unwrap().len(), 1);
        assert!(second.problem_id.is_none(), "one incident is below threshold");

        // an episode was recorded for the opened incident
        assert!(episode_count(&conn) >= 1);
    }

    #[test]
    fn monitor_recurrence_opens_and_links_problem() {
        let conn = test_conn();
        // three distinct incidents (closed between failures so a new one opens each time)
        let o1 = monitor_evaluate(&conn, "flapper", "error", "x", "low", 3).unwrap();
        incident_close(&conn, o1.incident_id.as_ref().unwrap()).unwrap();
        let o2 = monitor_evaluate(&conn, "flapper", "error", "x", "low", 3).unwrap();
        incident_close(&conn, o2.incident_id.as_ref().unwrap()).unwrap();
        assert!(o1.problem_id.is_none() && o2.problem_id.is_none());

        let o3 = monitor_evaluate(&conn, "flapper", "error", "x", "low", 3).unwrap();
        let pid = o3.problem_id.expect("threshold reached -> problem opened");

        // all three incidents are linked to the one problem
        let mut linked = problem_incident_ids(&conn, &pid).unwrap();
        let mut expected: Vec<String> = incidents_for_skill(&conn, "flapper")
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        linked.sort();
        expected.sort();
        assert_eq!(linked, expected);
        assert_eq!(problem_list(&conn, None).unwrap().len(), 1, "exactly one problem");

        // a fourth failure reuses the same problem (no second problem)
        let o4 = monitor_evaluate(&conn, "flapper", "error", "x", "low", 3).unwrap();
        assert_eq!(o4.problem_id.as_deref(), Some(pid.as_str()));
        assert_eq!(problem_list(&conn, None).unwrap().len(), 1);
    }

    // --- FEAT-035: ITIL schema extensions ---

    #[test]
    fn incident_block_sets_status_and_workaround() {
        let conn = test_conn();
        let inc = incident_open(&conn, "db slow", "high latency", "high", "monitor", Some("db")).unwrap();
        assert!(inc.workaround.is_none());

        incident_block(&conn, &inc.id, "serving from read replica").unwrap();
        let got = incident_get(&conn, &inc.id).unwrap().unwrap();
        assert_eq!(got.status, "blocked");
        assert_eq!(got.workaround.as_deref(), Some("serving from read replica"));
    }

    #[test]
    fn change_links_problem_and_runs_approval_gate() {
        let conn = test_conn();
        let prob = problem_open(&conn, "leak", "memory grows").unwrap();
        let chg = change_record(
            &conn,
            "bump heap + patch",
            "resolve the leak",
            None,
            Some(&prob.id),
            None,
            None,
        )
        .unwrap();
        assert_eq!(chg.status, "planned");
        assert_eq!(chg.problem_id.as_deref(), Some(prob.id.as_str()));
        assert!(chg.approved_by.is_none() && chg.approved_at.is_none());

        // planned -> pending_approval -> approved (back to planned, stamped)
        change_request_approval(&conn, &chg.id).unwrap();
        assert_eq!(change_get(&conn, &chg.id).unwrap().unwrap().status, "pending_approval");

        change_approve(&conn, &chg.id, "rene").unwrap();
        let approved = change_get(&conn, &chg.id).unwrap().unwrap();
        assert_eq!(approved.status, "planned");
        assert_eq!(approved.approved_by.as_deref(), Some("rene"));
        assert!(approved.approved_at.is_some());

        change_apply(&conn, &chg.id).unwrap();
        assert_eq!(change_get(&conn, &chg.id).unwrap().unwrap().status, "applied");
    }

    #[test]
    fn problem_hypotheses_round_trip() {
        let conn = test_conn();
        let prob = problem_open(&conn, "flaky deploys", "intermittent 500s").unwrap();
        assert!(hypothesis_list(&conn, &prob.id).unwrap().is_empty());

        let h1 = hypothesis_add(&conn, &prob.id, "connection pool exhausted").unwrap();
        let h2 = hypothesis_add(&conn, &prob.id, "DNS flapping").unwrap();
        assert_eq!(h1.status, "created");

        let list = hypothesis_list(&conn, &prob.id).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, h1.id, "oldest first");

        hypothesis_set_status(&conn, &h1.id, "validating").unwrap();
        hypothesis_set_status(&conn, &h1.id, "confirmed").unwrap();
        hypothesis_set_status(&conn, &h2.id, "rejected").unwrap();
        let list = hypothesis_list(&conn, &prob.id).unwrap();
        assert_eq!(list[0].status, "confirmed");
        assert_eq!(list[1].status, "rejected");
        assert!(list[0].updated_at >= list[0].created_at);
    }

    #[test]
    fn linkage_lives_in_join_and_is_queryable() {
        let conn = test_conn();
        let prob = problem_open(&conn, "p", "d").unwrap();
        let inc = incident_open(&conn, "i", "d", "low", "manual", None).unwrap();
        assert!(incident_problem_id(&conn, &inc.id).unwrap().is_none());
        link_incident_to_problem(&conn, &prob.id, &inc.id).unwrap();
        assert_eq!(incident_problem_id(&conn, &inc.id).unwrap().as_deref(), Some(prob.id.as_str()));
    }

    #[test]
    fn migrates_legacy_incident_problem_id_into_join_then_drops_column() {
        // Simulate a pre-FEAT-035 database: incidents carries a problem_id column.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE incidents (id TEXT PRIMARY KEY, title TEXT NOT NULL,
                description TEXT NOT NULL, severity TEXT NOT NULL, status TEXT NOT NULL,
                source TEXT NOT NULL, skill_name TEXT, problem_id TEXT, opened_at TEXT NOT NULL,
                updated_at TEXT NOT NULL, resolved_at TEXT, resolution TEXT);
             INSERT INTO incidents VALUES ('i1','t','d','low','open','manual',NULL,'p1',
                '2020-01-01','2020-01-01',NULL,NULL);",
        )
        .unwrap();

        // Running the schema migrates the legacy linkage and drops the column.
        init_schema(&conn).unwrap();

        let col: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('incidents') WHERE name = 'problem_id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(col, 0, "legacy incidents.problem_id column is dropped");
        assert_eq!(
            incident_problem_id(&conn, "i1").unwrap().as_deref(),
            Some("p1"),
            "legacy linkage folded into the join"
        );
        // Idempotent second pass.
        init_schema(&conn).unwrap();
    }
}
