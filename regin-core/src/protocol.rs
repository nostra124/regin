use serde::{Deserialize, Serialize};

use crate::types::{ChatMessage, Conversation, Memory, Schedule, SkillInfo, TaskRun};

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
}
