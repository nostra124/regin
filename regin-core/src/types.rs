use serde::{Deserialize, Serialize};

/// A chat message for the LLM API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// A conversation record from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A message record from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// A task run record from the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub id: String,
    pub skill_name: String,
    pub status: String,
    pub output: String,
    pub started_at: String,
    pub finished_at: String,
}

/// Summary info about a skill, for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    /// "system" or "user"
    pub source: String,
}

/// A scheduled task run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub skill: String,
    pub interval: String,
    pub next_run: String,
    pub last_run: Option<String>,
}

/// A memory entry — self-managed knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub category: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// ITIL records (FEAT-002)
// ---------------------------------------------------------------------------

/// An incident: an unplanned interruption or degradation.
/// status: open | investigating | resolved | closed
/// source: manual | monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: String,
    pub status: String,
    pub source: String,
    /// Skill that produced this incident, when source = monitor.
    pub skill_name: Option<String>,
    /// The problem this incident was linked to, if any.
    pub problem_id: Option<String>,
    pub opened_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
    pub resolution: Option<String>,
}

/// A change: a deliberate modification to a system.
/// status: planned | applied | closed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    /// The incident this change remediates, if any.
    pub incident_id: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub created_at: String,
    pub applied_at: Option<String>,
}

/// A problem: the underlying cause behind one or more incidents.
/// status: open | known_error | closed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub root_cause: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
}

/// An episodic-memory entry — the short-term record of *what happened*,
/// distilled into long-term (semantic) memories by reflection (FEAT-005/006).
/// kind: task_run | incident | chat | change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub kind: String,
    /// The related record's id (e.g. the task_run or incident id), if any.
    pub ref_id: Option<String>,
    pub summary: String,
    pub detail: Option<String>,
    pub created_at: String,
    /// Whether a reflection pass has already consumed this episode.
    pub reflected: bool,
}
