use serde::{Deserialize, Serialize};

use crate::desired::{DesiredInfo, DesiredState};
use crate::filters::FilterRule;
use crate::audit::Finding;
use crate::goal::Goal;
use crate::greeting::{Greeting, IntentRagSummary};
use crate::objective::Objective as StandingObjective;
use crate::promotion::DerivedCheck;
use crate::kpi::{KpiSummary, Objective};
use crate::types::{
    Change, ChatMessage, Conversation, Incident, Memory, Principle, Problem, ProblemHypothesis, Schedule,
    SkillInfo, TaskRun,
};

/// Request from CLI to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "skill_list")]
    SkillList { cwd: Option<String> },

    #[serde(rename = "skill_show")]
    SkillShow { name: String, cwd: Option<String> },

    /// Execute a task. `cwd` is the caller's working directory (for repo context).
    #[serde(rename = "task_exec")]
    TaskExec { skill: String, cwd: Option<String> },

    #[serde(rename = "task_schedule")]
    TaskSchedule { skill: String, interval: String },

    #[serde(rename = "task_unschedule")]
    TaskUnschedule { skill: String },

    #[serde(rename = "task_schedules")]
    TaskSchedules,

    /// Create a new skill. With `from_prompt`, the agent drafts the skill.md;
    /// otherwise a template is scaffolded. Refuses overwrite unless `force`.
    /// With `repo`, the skill is stored in regin's per-repo store keyed by `cwd`
    /// (FEAT-009) instead of the user skills dir.
    #[serde(rename = "task_create")]
    TaskCreate {
        name: String,
        from_prompt: Option<String>,
        force: bool,
        repo: bool,
        cwd: Option<String>,
    },

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

    /// Run one Hermes reflection pass now (FEAT-006).
    #[serde(rename = "memory_reflect")]
    MemoryReflect,

    /// Export identity database to a portable snapshot (FEAT-027).
    #[serde(rename = "memory_export")]
    MemoryExport { path: String },

    /// Import a portable identity snapshot (FEAT-027).
    #[serde(rename = "memory_import")]
    MemoryImport { path: String, merge: bool },

    /// Show identity metadata (FEAT-027).
    #[serde(rename = "memory_info")]
    MemoryInfo,

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
    /// Block an incident on a workaround while its problem awaits a fix (FEAT-035).
    #[serde(rename = "incident_block")]
    IncidentBlock { id: String, workaround: String },

    // --- ITIL: Changes ---
    #[serde(rename = "change_record")]
    ChangeRecord {
        title: String,
        description: String,
        incident_id: Option<String>,
        problem_id: Option<String>,
        before: Option<String>,
        after: Option<String>,
    },
    #[serde(rename = "change_list")]
    ChangeList,
    #[serde(rename = "change_show")]
    ChangeShow { id: String },
    /// Move a change to pending_approval (FEAT-035).
    #[serde(rename = "change_request_approval")]
    ChangeRequestApproval { id: String },
    /// Approve a pending change, recording the approver (FEAT-035).
    #[serde(rename = "change_approve")]
    ChangeApprove { id: String, approved_by: String },
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
    /// Add a root-cause hypothesis to a problem (FEAT-035).
    #[serde(rename = "problem_hypothesis_add")]
    ProblemHypothesisAdd { problem_id: String, text: String },
    /// List a problem's hypotheses (FEAT-035).
    #[serde(rename = "problem_hypothesis_list")]
    ProblemHypothesisList { problem_id: String },
    /// Set a hypothesis's status: created|validating|confirmed|rejected (FEAT-035).
    #[serde(rename = "problem_hypothesis_status")]
    ProblemHypothesisStatus { id: String, status: String },

    // --- Desired state (to-be) — FEAT-033 ---
    /// List loaded desired-state domains (with conflict flags).
    #[serde(rename = "desired_list")]
    DesiredList,
    /// Show one domain's desired state.
    #[serde(rename = "desired_show")]
    DesiredShow { domain: String },
    /// Re-check all desired states, opening problems for contradictory targets.
    #[serde(rename = "desired_check")]
    DesiredCheck,

    // --- CSI metrics (FEAT-050) ---
    /// Compute the KPI snapshot + constrained objective over the last N days.
    #[serde(rename = "metrics")]
    Metrics { since_days: Option<u32> },

    // --- Notice filters (FEAT-052) ---
    /// List loaded notice-filter rules (system + user).
    #[serde(rename = "filters_list")]
    FiltersList,
    /// Test whether an observation would be filtered.
    #[serde(rename = "filters_test")]
    FiltersTest { domain: String, text: String },

    // --- Effective mode (FEAT-041) ---
    /// Report regin's effective operating mode (org vs standalone).
    #[serde(rename = "mode")]
    ModeQuery,

    // --- Adaptive posture (FEAT-040) ---
    /// Report the current autonomy posture and the evidence behind it.
    #[serde(rename = "posture")]
    PostureQuery,

    // --- Login greeting (FEAT-043) ---
    /// The login greeting: health line + parked actionable items.
    #[serde(rename = "greeting")]
    GreetingQuery,

    // --- Active push (FEAT-044) ---
    /// Send a test notification over the configured push channel.
    #[serde(rename = "push_test")]
    PushTest,

    // --- Promoted deterministic checks (FEAT-051) ---
    /// List active derived (promoted) deterministic checks.
    #[serde(rename = "checks_list")]
    ChecksList,

    // --- Self-audit (FEAT-055) ---
    /// Run the periodic CSI self-audit now and file its findings.
    #[serde(rename = "audit_run")]
    AuditRun,

    // --- Soul configurator + value catalog (FEAT-030) ---
    /// Browse the bundled value catalog.
    #[serde(rename = "soul_values_list")]
    SoulValuesList,
    /// Show one catalog entry by id.
    #[serde(rename = "soul_values_show")]
    SoulValuesShow { id: String },
    /// Render the active grounding: core charter ∪ active Persona overlay.
    #[serde(rename = "soul_charter_show")]
    SoulCharterShow,
    /// Propose a starting value set for the active Persona's role
    /// (`role_default_values`) — preview only, nothing is written.
    #[serde(rename = "soul_charter_derive")]
    SoulCharterDerive,
    /// Write value ids into the identity-core charter (human confirmation of
    /// a derive proposal, or a direct `regin soul charter set`).
    #[serde(rename = "soul_charter_confirm")]
    SoulCharterConfirm { value_ids: Vec<String> },
    /// Remove a value from the identity-core charter.
    #[serde(rename = "soul_charter_remove")]
    SoulCharterRemove { value_id: String },

    // --- Principle derivation & ratification (FEAT-031) ---
    /// List principles: every one, or only `candidate`-status ones.
    #[serde(rename = "soul_principles_list")]
    SoulPrinciplesList { candidates_only: bool },
    /// Promote a candidate principle to `active` (human ratification).
    #[serde(rename = "soul_principles_ratify")]
    SoulPrinciplesRatify { id: String },
    /// Retire a principle — a candidate rejection or an active retirement.
    #[serde(rename = "soul_principles_reject")]
    SoulPrinciplesReject { id: String },

    // --- Intent plane: objectives & goals (FEAT-069) ---
    /// List standing objectives, priority-ordered.
    #[serde(rename = "objective_list")]
    ObjectiveList,
    /// Show one objective by id.
    #[serde(rename = "objective_show")]
    ObjectiveShow { id: String },
    /// List goals, optionally filtered by lifecycle status.
    #[serde(rename = "goal_list")]
    GoalList { status: Option<String> },
    /// Show one goal by id.
    #[serde(rename = "goal_show")]
    GoalShow { id: String },
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

    #[serde(rename = "hypotheses")]
    Hypotheses { hypotheses: Vec<ProblemHypothesis> },

    // --- Desired state (FEAT-033) ---
    #[serde(rename = "desired_list")]
    DesiredListResp { items: Vec<DesiredInfo> },

    #[serde(rename = "desired_detail")]
    DesiredDetail { state: Box<DesiredState> },

    #[serde(rename = "metrics")]
    Metrics { summary: Box<KpiSummary>, objective: Objective, intent_rag: IntentRagSummary },

    #[serde(rename = "filters")]
    Filters { rules: Vec<FilterRule> },

    #[serde(rename = "mode")]
    ModeInfo { mode: String, configured: bool, last_ok: Option<String>, failures: u32 },

    #[serde(rename = "posture")]
    PostureInfo {
        posture: String,
        allow_auto: bool,
        change_successes: i64,
        change_failures: i64,
        change_success_rate: f64,
        promotion_error_rate: f64,
    },

    #[serde(rename = "greeting")]
    GreetingResp { greeting: Box<Greeting> },

    #[serde(rename = "derived_checks")]
    DerivedChecks { checks: Vec<DerivedCheck> },

    #[serde(rename = "audit")]
    AuditResult { findings: Vec<Finding>, trimmed: bool, opened: usize },

    #[serde(rename = "context")]
    Context { repo_key: Option<String>, content: Option<String> },

    #[serde(rename = "skill_created")]
    SkillCreated { path: String, shadows_system: bool },

    #[serde(rename = "memory_export")]
    MemoryExport { path: String },

    #[serde(rename = "memory_info")]
    MemoryInfo {
        identity_id: String,
        name: String,
        host: String,
        schema_version: String,
        memory_count: i64,
        created_at: String,
    },

    #[serde(rename = "reflect_stats")]
    ReflectStats { episodes: u32, reinforced: u32, created: u32, decayed: u32 },

    // --- Soul configurator + value catalog (FEAT-030) ---
    #[serde(rename = "soul_values")]
    SoulValues { version: String, values: Vec<crate::soul::ValueEntry> },
    #[serde(rename = "soul_value_detail")]
    SoulValueDetail { value: crate::soul::ValueEntry },
    #[serde(rename = "soul_charter")]
    SoulCharter { core_ids: Vec<String>, persona_overlay: Vec<String>, grounding: Vec<String> },
    #[serde(rename = "soul_charter_proposal")]
    SoulCharterProposal { role: String, proposed: Vec<String> },
    #[serde(rename = "soul_charter_written")]
    SoulCharterWritten { added: Vec<String> },

    // --- Principle derivation & ratification (FEAT-031) ---
    #[serde(rename = "soul_principles")]
    SoulPrinciples { principles: Vec<Principle> },
    #[serde(rename = "soul_principle_ratified")]
    SoulPrincipleRatified { principle: Principle },
    #[serde(rename = "soul_principle_rejected")]
    SoulPrincipleRejected { principle: Principle },

    // --- Intent plane: objectives & goals (FEAT-069) ---
    #[serde(rename = "objectives")]
    Objectives { objectives: Vec<StandingObjective> },
    #[serde(rename = "objective_detail")]
    ObjectiveDetail { objective: Box<StandingObjective> },
    #[serde(rename = "goals")]
    Goals { goals: Vec<Goal> },
    #[serde(rename = "goal_detail")]
    GoalDetail { goal: Box<Goal> },
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
            problem_id: Some("p".into()),
            before: None,
            after: Some("up".into()),
        });
        req_roundtrip(&Request::IncidentBlock { id: "i".into(), workaround: "wa".into() });
        req_roundtrip(&Request::ChangeRequestApproval { id: "c".into() });
        req_roundtrip(&Request::ChangeApprove { id: "c".into(), approved_by: "rene".into() });
        req_roundtrip(&Request::ProblemHypothesisAdd { problem_id: "p".into(), text: "t".into() });
        req_roundtrip(&Request::ProblemHypothesisList { problem_id: "p".into() });
        req_roundtrip(&Request::ProblemHypothesisStatus { id: "h".into(), status: "confirmed".into() });
        req_roundtrip(&Request::ProblemLink { problem_id: "p".into(), incident_id: "i".into() });
        req_roundtrip(&Request::ProblemKnownError { id: "p".into(), root_cause: "rc".into() });
    }

    #[test]
    fn itil_request_tag_is_stable() {
        let j = serde_json::to_string(&Request::IncidentClose { id: "abc".into() }).unwrap();
        assert!(j.contains("\"type\":\"incident_close\""), "got {j}");
    }
}
