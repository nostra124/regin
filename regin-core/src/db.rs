use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::{debug, info};

use crate::types::{Conversation, Message, TaskRun};

/// Initialize the SQLite database at the given path, creating tables if needed.
pub fn init_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create database directory: {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("Failed to open database: {}", path.display()))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS conversations (
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
        );",
    )
    .context("Failed to create database tables")?;

    info!("Database initialized at {}", path.display());
    Ok(conn)
}

/// Save a message to the database. Creates the conversation if it doesn't exist.
pub fn save_message(
    conn: &Connection,
    conversation_id: &str,
    conversation_title: &str,
    role: &str,
    content: &str,
) -> Result<Message> {
    let now = chrono::Utc::now().to_rfc3339();
    let msg_id = uuid::Uuid::new_v4().to_string();

    // Upsert conversation
    conn.execute(
        "INSERT INTO conversations (id, title, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET updated_at = ?4",
        params![conversation_id, conversation_title, &now, &now],
    )
    .context("Failed to upsert conversation")?;

    // Insert message
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

/// Get all messages for a conversation, ordered by creation time.
pub fn get_conversation_messages(conn: &Connection, conversation_id: &str) -> Result<Vec<Message>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, role, content, created_at
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at ASC",
        )
        .context("Failed to prepare message query")?;

    let messages = stmt
        .query_map(params![conversation_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .context("Failed to query messages")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read message rows")?;

    debug!(conversation_id, count = messages.len(), "Messages retrieved");
    Ok(messages)
}

/// List all conversations, ordered by most recently updated first.
pub fn list_conversations(conn: &Connection) -> Result<Vec<Conversation>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, created_at, updated_at
             FROM conversations
             ORDER BY updated_at DESC",
        )
        .context("Failed to prepare conversation query")?;

    let conversations = stmt
        .query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .context("Failed to query conversations")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read conversation rows")?;

    debug!(count = conversations.len(), "Conversations listed");
    Ok(conversations)
}

/// Save a task run record to the database.
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
    )
    .context("Failed to insert task run")?;

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

/// Get recent task runs for a skill, ordered by most recent first.
pub fn get_task_runs(conn: &Connection, skill_name: &str, limit: usize) -> Result<Vec<TaskRun>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, skill_name, status, output, started_at, finished_at
             FROM task_runs
             WHERE skill_name = ?1
             ORDER BY started_at DESC
             LIMIT ?2",
        )
        .context("Failed to prepare task run query")?;

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
        })
        .context("Failed to query task runs")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read task run rows")?;

    debug!(skill_name, count = runs.len(), "Task runs retrieved");
    Ok(runs)
}
