use serde::{Deserialize, Serialize};

use crate::types::{
    Change, ChatMessage, Conversation, Incident, Memory, Problem, Schedule, SkillInfo, TaskRun,
};

/// Request from CLI to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "skill_list")]
    SkillList,

    #[serde(rename = "skill_show")]
    SkillShow { name: String },

    /// Execute a task. `cwd` is the caller's working directory (for repo context).
    #[serde(rename = "task_exec")]
    TaskExec { skill: String, cwd: Option<String> },

    #[serde(rename = "task_schedule")]
    TaskSchedule { skill: String, interval: String },

    #[serde(rename = "task_unschedule")]
    TaskUnschedule { skill: String },

    #[serde(rename = "task_schedules")]
    TaskSchedules,

    /// Create a new user skill. With `from_prompt`, the agent drafts the skill.md;
    /// otherwise a template is scaffolded. Refuses overwrite unless `force`.
    #[serde(rename = "task_create")]
    TaskCreate { name: String, from_prompt: Option<String>, force: bool },

    #[serde(rename = "runs_list")]
    RunsList { skill: Option<String>, limit: u32 },

    /// Chat message. `cwd` for repo context lookup.
    #[serde(rename = "chat_send")]
    ChatSend {
        conversation_id: String,
        messages: Vec<ChatMessage>,
        cwd: Option<String>,
    },

    #[serde(rename = "chat_new")]
    ChatNew,

    #[serde(rename = "chat_history")]
    ChatHistory,

    #[serde(rename = "config_list")]
    ConfigList,

    #[serde(rename = "config_get")]
    ConfigGet { key: String },

    #[serde(rename = "config_set")]
    ConfigSet { key: String, value: String },

    // --- Memory ---
    #[serde(rename = "memory_list")]
    MemoryList { category: Option<String> },

    #[serde(rename = "memory_search")]
    MemorySearch { query: String },

    #[serde(rename = "memory_save")]
    MemorySave { category: String, content: String },

    #[serde(rename = "memory_update")]
    MemoryUpdate { id: String, content: String },

    #[serde(rename = "memory_delete")]
    MemoryDelete { id: String },

    // --- Per-repo context (FEAT-008) ---
    #[serde(rename = "context_show")]
    ContextShow { cwd: Option<String> },

    #[serde(rename = "context_set")]
    ContextSet { cwd: Option<String>, content: String },

    // --- ITIL: Incidents ---
    #[serde(rename = "incident_open")]
    IncidentOpen { title: String, description: String, severity: String },
    #[serde(rename = "incident_list")]
    IncidentList { status: Option<String> },
    #[serde(rename = "incident_show")]
    IncidentShow { id: String },
    #[serde(rename = "incident_update")]
    IncidentUpdate { id: String, status: String },
    #[serde(rename = "incident_resolve")]
    IncidentResolve { id: String, resolution: String },
    #[serde(rename = "incident_close")]
    IncidentClose { id: String },

    // --- ITIL: Changes ---
    #[serde(rename = "change_record")]
    ChangeRecord {
        title: String,
        description: String,
        incident_id: Option<String>,
        before: Option<String>,
        after: Option<String>,
    },
    #[serde(rename = "change_list")]
    ChangeList,
    #[serde(rename = "change_show")]
    ChangeShow { id: String },
    #[serde(rename = "change_apply")]
    ChangeApply { id: String },
    #[serde(rename = "change_close")]
    ChangeClose { id: String },

    // --- ITIL: Problems ---
    #[serde(rename = "problem_open")]
    ProblemOpen { title: String, description: String },
    #[serde(rename = "problem_list")]
    ProblemList { status: Option<String> },
    #[serde(rename = "problem_show")]
    ProblemShow { id: String },
    #[serde(rename = "problem_link")]
    ProblemLink { problem_id: String, incident_id: String },
    #[serde(rename = "problem_known_error")]
    ProblemKnownError { id: String, root_cause: String },
    #[serde(rename = "problem_close")]
    ProblemClose { id: String },
}

/// Response from daemon to CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "ok")]
    Ok { message: String },

    #[serde(rename = "error")]
    Error { message: String },

    #[serde(rename = "pong")]
    Pong,

    #[serde(rename = "skill_list")]
    SkillList { skills: Vec<SkillInfo> },

    #[serde(rename = "skill_detail")]
    SkillDetail {
        name: String,
        description: String,
        prompt: String,
        files: Vec<String>,
    },

    #[serde(rename = "task_result")]
    TaskResult { run: TaskRun },

    #[serde(rename = "runs_list")]
    RunsList { runs: Vec<TaskRun> },

    #[serde(rename = "schedules_list")]
    SchedulesList { schedules: Vec<Schedule> },

    /// A streamed text token from the LLM.
    #[serde(rename = "stream_chunk")]
    StreamChunk { token: String },

    /// The LLM is calling a tool.
    #[serde(rename = "tool_call")]
    ToolCallEvent { name: String, arguments: String },

    /// Tool execution result.
    #[serde(rename = "tool_result")]
    ToolResultEvent { name: String, success: bool, output: String },

    /// Stream/agentic loop finished.
    #[serde(rename = "stream_done")]
    StreamDone { conversation_id: String },

    #[serde(rename = "chat_new")]
    ChatNew { conversation_id: String },

    #[serde(rename = "chat_history")]
    ChatHistory { conversations: Vec<Conversation> },

    #[serde(rename = "config_entries")]
    ConfigEntries { entries: Vec<(String, String)> },

    #[serde(rename = "config_value")]
    ConfigValue { key: String, value: String },

    #[serde(rename = "memory_list")]
    MemoryList { memories: Vec<Memory> },

    // --- ITIL ---
    #[serde(rename = "incidents")]
    Incidents { incidents: Vec<Incident> },

    #[serde(rename = "changes")]
    Changes { changes: Vec<Change> },

    #[serde(rename = "problems")]
    Problems { problems: Vec<Problem> },

    #[serde(rename = "context")]
    Context { repo_key: Option<String>, content: Option<String> },

    #[serde(rename = "skill_created")]
    SkillCreated { path: String, shadows_system: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req_roundtrip(r: &Request) {
        let j = serde_json::to_string(r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        assert_eq!(format!("{r:?}"), format!("{back:?}"));
    }

    #[test]
    fn itil_requests_roundtrip() {
        req_roundtrip(&Request::IncidentOpen {
            title: "t".into(),
            description: "d".into(),
            severity: "high".into(),
        });
        req_roundtrip(&Request::IncidentList { status: Some("open".into()) });
        req_roundtrip(&Request::IncidentResolve { id: "x".into(), resolution: "fixed".into() });
        req_roundtrip(&Request::ChangeRecord {
            title: "c".into(),
            description: "".into(),
            incident_id: Some("i".into()),
            before: None,
            after: Some("up".into()),
        });
        req_roundtrip(&Request::ProblemLink { problem_id: "p".into(), incident_id: "i".into() });
        req_roundtrip(&Request::ProblemKnownError { id: "p".into(), root_cause: "rc".into() });
    }

    #[test]
    fn itil_request_tag_is_stable() {
        let j = serde_json::to_string(&Request::IncidentClose { id: "abc".into() }).unwrap();
        assert!(j.contains("\"type\":\"incident_close\""), "got {j}");
    }
}
