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
    /// Reinforcement count: how strongly this memory is held (FEAT-006).
    #[serde(default = "one")]
    pub strength: i64,
    /// When reflection last reinforced this memory.
    #[serde(default)]
    pub last_seen: Option<String>,
    /// `human` (hand-saved, never auto-decayed) or `reflection` (auto-distilled).
    #[serde(default = "human_source")]
    pub source: String,
}

fn one() -> i64 {
    1
}
fn human_source() -> String {
    "human".to_string()
}

// ---------------------------------------------------------------------------
// ITIL records (FEAT-002)
// ---------------------------------------------------------------------------

/// An incident: an unplanned interruption or degradation.
/// status: open | investigating | blocked | resolved | closed
/// source: manual | monitor
///
/// `blocked` (FEAT-035) parks an incident on a `workaround` while the underlying
/// problem awaits a real fix. The problem linkage lives only in the
/// `problem_incidents` join table (the redundant `incidents.problem_id` column was
/// dropped per DISC-011) — see [`crate::db::incident_problem_id`].
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
    /// A temporary workaround keeping things running while `blocked` (FEAT-035).
    pub workaround: Option<String>,
    pub opened_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
    pub resolution: Option<String>,
}

/// A change: a deliberate modification to a system.
/// status: planned | pending_approval | applied | closed
///
/// A change may remediate an incident *or* resolve a problem (`problem_id`,
/// FEAT-035). `pending_approval` sits between `planned` and `applied`: the change
/// is staged but awaits a human/supervisor decision, recorded in `approved_by` /
/// `approved_at` on approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    /// The incident this change remediates, if any.
    pub incident_id: Option<String>,
    /// The problem this change resolves, if any (FEAT-035).
    pub problem_id: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    /// Who approved the change out of `pending_approval` (FEAT-035).
    pub approved_by: Option<String>,
    /// When the change was approved (FEAT-035).
    pub approved_at: Option<String>,
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

/// A hypothesis about a problem's root cause (FEAT-035).
/// status: created | validating | confirmed | rejected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemHypothesis {
    pub id: String,
    pub problem_id: String,
    pub text: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A session row from `identity.db` (FEAT-023).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub host: Option<String>,
    pub kind: String,
    pub title: String,
    pub message_count: i64,
    pub token_count: i64,
    pub state: String,
    /// First 200 characters of the transcript text.
    pub transcript_preview: Option<String>,
    pub summary: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}

/// A single transcript message (FEAT-023).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// A session with its full transcript (FEAT-023).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWithTranscript {
    pub session: SessionRow,
    pub messages: Vec<TranscriptMessage>,
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

/// Action the LLM proposes during curation (FEAT-024).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CuratorAction {
    Add,
    Update,
    Delete,
    Noop,
}

/// A single curator proposal from the LLM (FEAT-024).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratorProposal {
    pub action: CuratorAction,
    pub category: String,
    pub content: String,
    /// Memory id to UPDATE/DELETE (ignored for Add/Noop).
    pub target_id: Option<String>,
    /// Topic slug to assign (empty = no topic).
    pub topic: Option<String>,
    /// Optional tags for classification.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Outcome of a curation pass (FEAT-024).
#[derive(Debug, Clone, Default)]
pub struct CuratorStats {
    pub episodes: usize,
    pub sessions: usize,
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub noop: usize,
    pub promoted: usize,
    pub decayed: usize,
    pub pruned: usize,
    pub topics: usize,
}
