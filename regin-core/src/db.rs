use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::{debug, info};

use crate::types::{Change, Conversation, Incident, Memory, Message, Problem, Schedule, TaskRun};

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
            updated_at TEXT NOT NULL
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
            problem_id TEXT,
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
            before_state TEXT,
            after_state TEXT,
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
        );",
    )
    .context("Failed to create database tables")?;

    // Seed defaults for any missing settings
    for (key, default, _desc) in crate::config::SETTINGS {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, default],
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

pub fn memory_save(conn: &Connection, category: &str, content: &str) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO memories (id, category, content, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, category, content, &now, &now],
    )?;
    Ok(Memory { id, category: category.into(), content: content.into(), created_at: now.clone(), updated_at: now })
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
    let (sql, p): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match category {
        Some(cat) => (
            "SELECT id, category, content, created_at, updated_at FROM memories WHERE category = ?1 ORDER BY updated_at DESC",
            vec![Box::new(cat.to_string())],
        ),
        None => (
            "SELECT id, category, content, created_at, updated_at FROM memories ORDER BY category, updated_at DESC",
            vec![],
        ),
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), |row| {
            Ok(Memory {
                id: row.get(0)?,
                category: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn memory_search(conn: &Connection, query: &str) -> Result<Vec<Memory>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, category, content, created_at, updated_at FROM memories
         WHERE content LIKE ?1 OR category LIKE ?1 ORDER BY updated_at DESC LIMIT 50",
    )?;
    let rows = stmt
        .query_map(params![&pattern], |row| {
            Ok(Memory {
                id: row.get(0)?,
                category: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
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
    "id, title, description, severity, status, source, skill_name, problem_id, \
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
        problem_id: row.get(7)?,
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
            (id, title, description, severity, status, source, skill_name, problem_id,
             opened_at, updated_at, resolved_at, resolution)
         VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6, NULL, ?7, ?7, NULL, NULL)",
        params![&id, title, description, severity, source, skill_name, &now],
    )?;
    debug!(id, title, "Incident opened");
    incident_get(conn, &id)?.context("incident vanished after insert")
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
    "id, title, description, status, incident_id, before_state, after_state, created_at, applied_at";

fn row_to_change(row: &rusqlite::Row) -> rusqlite::Result<Change> {
    Ok(Change {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        incident_id: row.get(4)?,
        before: row.get(5)?,
        after: row.get(6)?,
        created_at: row.get(7)?,
        applied_at: row.get(8)?,
    })
}

/// Record a change (status = planned).
pub fn change_record(
    conn: &Connection,
    title: &str,
    description: &str,
    incident_id: Option<&str>,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<Change> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO changes
            (id, title, description, status, incident_id, before_state, after_state, created_at, applied_at)
         VALUES (?1, ?2, ?3, 'planned', ?4, ?5, ?6, ?7, NULL)",
        params![&id, title, description, incident_id, before, after, &now],
    )?;
    change_get(conn, &id)?.context("change vanished after insert")
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

/// Link an incident to a problem (idempotent) and set the incident's problem_id.
pub fn link_incident_to_problem(conn: &Connection, problem_id: &str, incident_id: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO problem_incidents (problem_id, incident_id) VALUES (?1, ?2)",
        params![problem_id, incident_id],
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE incidents SET problem_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![problem_id, &now, incident_id],
    )?;
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
        assert_eq!(incident_get(&conn, &i1.id).unwrap().unwrap().problem_id.as_deref(), Some(prob.id.as_str()));

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
}
