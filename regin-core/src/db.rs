use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::{debug, info};

use crate::types::{Conversation, Memory, Message, Schedule, TaskRun};

/// Initialize the SQLite database at the given path, creating tables if needed.
pub fn init_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open database: {}", path.display()))?;

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

    info!("Database initialized at {}", path.display());
    Ok(conn)
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
