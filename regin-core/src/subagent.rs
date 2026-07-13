//! FEAT-079 (DISC-021): multi-agent orchestration. The `task` tool lets the
//! primary agent delegate a sub-task to a child session (subagent) — a
//! restricted-tool-set, own-conversation-history LLM loop, for parallel
//! exploration/research/work.
//!
//! Subagents share the parent's tool executor (same repo, same undo/lsp/db
//! state — FEAT-078/085) but never spawn further children: `"task"` is
//! excluded from every subagent's tool set by construction, giving the
//! "one level of nesting" rule (acceptance criterion 2) with no runtime
//! check to get wrong.

use crate::llm::{LlmClient, LlmTurn};
use crate::persona::Persona;
use crate::tools::{ToolCall, ToolDef, ToolResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

/// A named subagent role: system-prompt override + tool allowlist.
#[derive(Debug, Clone, PartialEq)]
pub struct SubagentType {
    pub name: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
}

/// The three built-in subagent types (acceptance criterion 3). `general`'s
/// tool list is every known tool except `task` itself — a subagent can
/// never spawn a grandchild.
pub fn built_in_types() -> Vec<SubagentType> {
    vec![
        SubagentType {
            name: "explore".into(),
            system_prompt: "You are a fast, read-only codebase search subagent. Use glob/grep/read_file to locate what was asked for and report exactly what you found: file paths, line numbers, and the relevant snippets. Do not attempt to modify anything.".into(),
            tools: vec!["glob".into(), "grep".into(), "read_file".into()],
        },
        SubagentType {
            name: "general".into(),
            system_prompt: "You are a general-purpose subagent handling one delegated task. Use whatever tools you need to complete it, then report the result clearly and concisely.".into(),
            tools: general_subagent_tools(),
        },
        SubagentType {
            name: "scout".into(),
            system_prompt: "You are a read-only research subagent. Use glob/grep/read_file to inspect the local repo and web_search for external research. Report your findings; do not modify anything.".into(),
            tools: vec!["glob".into(), "grep".into(), "read_file".into(), "web_search".into()],
        },
    ]
}

/// Resolve a subagent type by name (acceptance criterion 4): an
/// `agent.<name>.tools` setting override (comma-separated tool names, with
/// `agent.<name>.prompt` for its system prompt) takes precedence over a
/// built-in type of the same name; otherwise falls back to the built-in
/// type. `None` means no such type, built-in or configured.
pub fn resolve_subagent_type(conn: &rusqlite::Connection, name: &str) -> Result<Option<SubagentType>> {
    let tools_key = format!("agent.{name}.tools");
    let configured = crate::db::setting_get(conn, &tools_key)?;
    if !configured.trim().is_empty() {
        let prompt_key = format!("agent.{name}.prompt");
        let system_prompt = crate::db::setting_get(conn, &prompt_key)?;
        let tools = configured
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty() && *t != "task")
            .map(str::to_string)
            .collect();
        return Ok(Some(SubagentType { name: name.to_string(), system_prompt, tools }));
    }
    Ok(built_in_types().into_iter().find(|t| t.name == name))
}

/// Intersect a subagent type's tool list with the parent's capability
/// ceiling (defense in depth: a child can never exceed what its own parent
/// is allowed to do) and always exclude `task` itself.
pub fn effective_tools(sub: &SubagentType, parent_ceiling: Option<&Persona>) -> Vec<String> {
    sub.tools
        .iter()
        .filter(|t| t.as_str() != "task")
        .filter(|t| crate::persona::allows(parent_ceiling, t))
        .cloned()
        .collect()
}

/// A tool-execution callback for the subagent loop. Implemented in `regind`
/// (it needs the daemon's shared undo/lsp/db state); kept as a trait here so
/// [`run_subagent`] itself is testable with a fake, no daemon required.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, call: &ToolCall) -> ToolResult;
}

/// Bounds a runaway subagent loop. Defensive: a delegated task should
/// resolve well within this; if it doesn't, that's a bug in the subagent's
/// prompt or tools, not something worth spinning forever on.
pub const MAX_SUBAGENT_ROUNDS: usize = 25;

/// Run one subagent conversation to completion (acceptance criterion 1):
/// system prompt + the delegated task prompt, looping tool calls through
/// `executor` until the LLM returns final text (acceptance criterion 5) or
/// [`MAX_SUBAGENT_ROUNDS`] is exceeded.
pub async fn run_subagent(
    client: &dyn LlmClient,
    executor: &dyn ToolExecutor,
    sub: &SubagentType,
    tool_defs: &[ToolDef],
    task_prompt: &str,
) -> Result<String> {
    let mut msgs: Vec<Value> = Vec::new();
    if !sub.system_prompt.is_empty() {
        msgs.push(json!({ "role": "system", "content": sub.system_prompt }));
    }
    msgs.push(json!({ "role": "user", "content": task_prompt }));

    let tools = if tool_defs.is_empty() { None } else { Some(tool_defs) };

    for _ in 0..MAX_SUBAGENT_ROUNDS {
        let turn = client.chat_turn(&msgs, tools).await.context("subagent LLM turn")?;
        match turn {
            LlmTurn::ToolCalls { assistant_message, calls } => {
                msgs.push(assistant_message);
                for call in &calls {
                    let result = executor.execute(call).await;
                    msgs.push(crate::llm::MimirClient::tool_result_message(&result.tool_call_id, &result.output));
                }
            }
            LlmTurn::Text(text) => return Ok(text),
        }
    }
    anyhow::bail!("subagent exceeded {MAX_SUBAGENT_ROUNDS} tool-call rounds without a final answer")
}

/// Bounds how many subagents may run concurrently (acceptance criterion 6,
/// `task.max_concurrency`, default 3). Sized once at construction — like
/// other process-lifetime-scoped resources (e.g. the LSP pool), changing
/// `task.max_concurrency` takes effect on daemon restart.
pub struct TaskLimiter {
    semaphore: tokio::sync::Semaphore,
}

impl TaskLimiter {
    pub fn new(max_concurrency: usize) -> Self {
        Self { semaphore: tokio::sync::Semaphore::new(max_concurrency.max(1)) }
    }

    /// Acquire a permit, waiting if the limiter is already at capacity. The
    /// returned permit releases its slot on drop.
    pub async fn acquire(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.semaphore.acquire().await.expect("TaskLimiter semaphore is never closed")
    }
}

/// Every known tool except `task` itself — the `general` subagent type's
/// default tool list.
fn general_subagent_tools() -> Vec<String> {
    crate::persona::ALL_TOOLS.iter().filter(|t| **t != "task").map(|t| t.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::FunctionCall;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_schema(&c).unwrap();
        c
    }

    // --- built-in types + resolution (acceptance criteria 3, 4) -----------

    #[test]
    fn built_in_types_cover_explore_general_and_scout() {
        let types = built_in_types();
        let explore = types.iter().find(|t| t.name == "explore").unwrap();
        assert_eq!(explore.tools, vec!["glob", "grep", "read_file"]);

        let general = types.iter().find(|t| t.name == "general").unwrap();
        assert!(general.tools.contains(&"bash".to_string()));
        assert!(general.tools.contains(&"write_file".to_string()));
        assert!(!general.tools.contains(&"task".to_string()), "general must not be able to spawn a grandchild");

        let scout = types.iter().find(|t| t.name == "scout").unwrap();
        assert_eq!(scout.tools, vec!["glob", "grep", "read_file", "web_search"]);
    }

    #[test]
    fn resolve_falls_back_to_a_built_in_type() {
        let c = conn();
        let explore = resolve_subagent_type(&c, "explore").unwrap().unwrap();
        assert_eq!(explore.tools, vec!["glob", "grep", "read_file"]);
    }

    #[test]
    fn resolve_is_none_for_an_unconfigured_unknown_type() {
        let c = conn();
        assert!(resolve_subagent_type(&c, "reviewer").unwrap().is_none());
    }

    #[test]
    fn resolve_prefers_a_configured_custom_type() {
        let c = conn();
        crate::db::setting_set(&c, "agent.reviewer.tools", "glob, grep, read_file").unwrap();
        crate::db::setting_set(&c, "agent.reviewer.prompt", "You review diffs.").unwrap();
        let reviewer = resolve_subagent_type(&c, "reviewer").unwrap().unwrap();
        assert_eq!(reviewer.name, "reviewer");
        assert_eq!(reviewer.tools, vec!["glob", "grep", "read_file"]);
        assert_eq!(reviewer.system_prompt, "You review diffs.");
    }

    #[test]
    fn resolve_strips_task_from_a_configured_type_even_if_listed() {
        let c = conn();
        crate::db::setting_set(&c, "agent.sneaky.tools", "task, bash").unwrap();
        let sneaky = resolve_subagent_type(&c, "sneaky").unwrap().unwrap();
        assert_eq!(sneaky.tools, vec!["bash"]);
    }

    #[test]
    fn a_configured_type_overrides_a_built_in_of_the_same_name() {
        let c = conn();
        crate::db::setting_set(&c, "agent.explore.tools", "bash").unwrap();
        let explore = resolve_subagent_type(&c, "explore").unwrap().unwrap();
        assert_eq!(explore.tools, vec!["bash"]);
    }

    // --- effective_tools: parent-ceiling intersection (defense in depth) --

    #[test]
    fn effective_tools_excludes_task_and_respects_no_parent_ceiling() {
        let sub = SubagentType { name: "general".into(), system_prompt: String::new(), tools: vec!["bash".into(), "task".into()] };
        let tools = effective_tools(&sub, None);
        assert_eq!(tools, vec!["bash"]);
    }

    #[test]
    fn effective_tools_never_exceeds_the_parent_ceiling() {
        let parent = Persona::from_toml("role = \"scoped\"\ntools = [\"glob\", \"grep\"]\n").unwrap();
        let sub = SubagentType { name: "general".into(), system_prompt: String::new(), tools: vec!["bash".into(), "grep".into()] };
        let tools = effective_tools(&sub, Some(&parent));
        assert_eq!(tools, vec!["grep"], "bash is outside the parent's own ceiling, so the child can't have it either");
    }

    // --- run_subagent (acceptance criteria 1, 5) ---------------------------

    struct SpyExecutor {
        calls: Mutex<Vec<String>>,
    }

    impl SpyExecutor {
        fn new() -> Self {
            Self { calls: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl ToolExecutor for SpyExecutor {
        async fn execute(&self, call: &ToolCall) -> ToolResult {
            self.calls.lock().unwrap().push(call.function.name.clone());
            ToolResult { tool_call_id: call.id.clone(), name: call.function.name.clone(), output: "ok".into(), success: true }
        }
    }

    fn a_tool_call(id: &str, name: &str) -> ToolCall {
        ToolCall { id: id.into(), call_type: "function".into(), function: FunctionCall { name: name.into(), arguments: "{}".into() } }
    }

    #[tokio::test]
    async fn run_subagent_returns_final_text_with_no_tool_calls() {
        let llm = crate::llm::FakeLlm::new();
        llm.push_turn(LlmTurn::Text("done".into()));
        let executor = SpyExecutor::new();
        let sub = &built_in_types()[0];
        let result = run_subagent(&llm, &executor, sub, &[], "find the config loader").await.unwrap();
        assert_eq!(result, "done");
        assert!(executor.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_subagent_executes_tool_calls_through_the_executor_then_returns_text() {
        let llm = crate::llm::FakeLlm::new();
        llm.push_turn(LlmTurn::ToolCalls {
            assistant_message: json!({"role": "assistant", "tool_calls": []}),
            calls: vec![a_tool_call("call-1", "grep")],
        });
        llm.push_turn(LlmTurn::Text("found it in config.rs".into()));
        let executor = SpyExecutor::new();
        let sub = &built_in_types()[0];
        let result = run_subagent(&llm, &executor, sub, &[], "find the config loader").await.unwrap();
        assert_eq!(result, "found it in config.rs");
        assert_eq!(*executor.calls.lock().unwrap(), vec!["grep".to_string()]);
    }

    #[tokio::test]
    async fn run_subagent_gives_up_after_max_rounds() {
        let llm = crate::llm::FakeLlm::new();
        for i in 0..MAX_SUBAGENT_ROUNDS {
            llm.push_turn(LlmTurn::ToolCalls {
                assistant_message: json!({"role": "assistant", "tool_calls": []}),
                calls: vec![a_tool_call(&format!("call-{i}"), "grep")],
            });
        }
        let executor = SpyExecutor::new();
        let sub = &built_in_types()[0];
        let err = run_subagent(&llm, &executor, sub, &[], "loop forever").await.unwrap_err();
        assert!(err.to_string().contains("exceeded"));
        assert_eq!(executor.calls.lock().unwrap().len(), MAX_SUBAGENT_ROUNDS);
    }

    // --- TaskLimiter: concurrency enforcement (acceptance criterion 6) ----

    #[tokio::test]
    async fn task_limiter_never_exceeds_its_max_concurrency() {
        let limiter = std::sync::Arc::new(TaskLimiter::new(2));
        let active = std::sync::Arc::new(AtomicUsize::new(0));
        let peak = std::sync::Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..6 {
            let limiter = limiter.clone();
            let active = active.clone();
            let peak = peak.clone();
            handles.push(tokio::spawn(async move {
                let _permit = limiter.acquire().await;
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert!(peak.load(Ordering::SeqCst) <= 2, "never more than 2 concurrent subagents");
        assert_eq!(active.load(Ordering::SeqCst), 0, "all tasks completed");
    }
}
