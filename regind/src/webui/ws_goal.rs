//! Goal WebSocket (FEAT-087, acceptance criterion 7).
//!
//! **Scope decision** (see the session's Track B research pass): the
//! dormant MILESTONE-0.7.0 goal/task_network/rcpsp/task_executor/
//! control_loop pipeline (FEAT-060..069) has zero production call sites
//! anywhere in `regind` — wiring it up here would mean writing an
//! `LlmTaskPlanner`, an LLM-backed `GoalJudge`, and an `ActionRunner` from
//! scratch, a body of work on the order of a separate ticket. Instead,
//! this reuses the *same* proven agentic tool-calling loop `ws_chat`
//! already wires up, with a lightweight plan-then-execute framing:
//!
//! 1. One LLM call producing a JSON list of plan steps, sent to the client
//!    as `{"type":"plan","steps":[...]}`.
//! 2. A real tool-calling loop (the exact same machinery as `ws_chat`)
//!    emitting `{"type":"step_start"}` / `{"type":"tool_call"}` /
//!    `{"type":"tool_result"}` per round, ending with
//!    `{"type":"done","summary":"..."}` once the model stops calling tools.
//!
//! The FEAT-060..069 modules remain available for a future, different use
//! case (an autonomous background daemon-driven goal executor) — not this
//! interactive web session.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use super::{AuthedUser, SharedState, WebuiState};
use regin_core::{llm::LlmTurn, mcp, permission, persona::Persona, plugin, tools, types::ChatMessage};

pub async fn handler(ws: WebSocketUpgrade, State(state): SharedState, _user: AuthedUser) -> impl IntoResponse {
    ws.on_upgrade(move |socket| run(socket, state))
}

#[derive(Deserialize)]
struct StartGoal {
    goal: String,
    #[serde(default)]
    cwd: Option<String>,
}

async fn run(mut socket: WebSocket, state: Arc<WebuiState>) {
    let Some(Ok(Message::Text(text))) = socket.recv().await else { return };
    let start: StartGoal = match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            let _ = send_json(&mut socket, &json!({"type": "error", "message": format!("invalid start message: {e}")})).await;
            return;
        }
    };

    if let Err(e) = run_goal(&state, &start.goal, start.cwd.as_deref(), &mut socket).await {
        let _ = send_json(&mut socket, &json!({"type": "error", "message": format!("{e:#}")})).await;
    }
}

async fn run_goal(state: &Arc<WebuiState>, goal: &str, cwd: Option<&str>, socket: &mut WebSocket) -> anyhow::Result<()> {
    let app = &state.app;
    let client = app.llm_client()?;

    // Phase 1: a single, tool-free call asking for a short numbered plan.
    let plan_prompt = format!(
        "You are planning how to accomplish the following goal using a coding agent's tools (bash, read_file, write_file, edit_file, glob, grep, and others). \
         Respond with ONLY a JSON array of short imperative step strings (no prose, no markdown fence), e.g. [\"Read the config file\", \"Add the new setting\"]. \
         Goal: {goal}"
    );
    let plan_text = client.chat_completion(&[ChatMessage::user(plan_prompt)]).await.unwrap_or_default();
    let steps = parse_plan_steps(&plan_text);
    send_json(socket, &json!({"type": "plan", "steps": steps})).await?;

    // Phase 2: the same tool-calling loop `ws_chat` uses, seeded with the
    // goal + the plan we just showed the client, so the model has that
    // context without needing to re-derive it.
    let persona = Persona::from_env().unwrap_or(None);
    let mut tool_defs = tools::tool_definitions_for(persona.as_ref());
    if persona.as_ref().is_none_or(|p| p.tools.is_empty()) {
        tool_defs.extend(app.mcp.tool_definitions());
    }

    let mut msgs: Vec<Value> = Vec::new();
    if let Some(p) = &persona
        && !p.prompt.is_empty()
    {
        msgs.push(json!({ "role": "system", "content": p.prompt }));
    }
    msgs.extend(crate::build_context(app, cwd));
    let steps_list = steps.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n");
    msgs.push(json!({
        "role": "user",
        "content": format!("Goal: {goal}\n\nPlan:\n{steps_list}\n\nExecute this plan using the available tools. When finished, summarize what you did.")
    }));

    loop {
        let turn = client.chat_turn(&msgs, Some(&tool_defs)).await?;
        match turn {
            LlmTurn::ToolCalls { assistant_message, calls } => {
                msgs.push(assistant_message);
                for call in &calls {
                    send_json(socket, &json!({"type": "tool_call", "name": call.function.name, "arguments": call.function.arguments})).await?;

                    let before = app.plugins.tool_execute_before(&call.function.name, &call.function.arguments);
                    let mut result = match before {
                        plugin::ToolBeforeAction::Reject { reason } => tools::ToolResult {
                            tool_call_id: call.id.clone(),
                            name: call.function.name.clone(),
                            output: format!("Blocked by plugin: {reason}"),
                            success: false,
                        },
                        plugin::ToolBeforeAction::Continue { args } => {
                            let rewritten;
                            let effective_call = if args == call.function.arguments {
                                call
                            } else {
                                rewritten = tools::ToolCall {
                                    id: call.id.clone(),
                                    call_type: call.call_type.clone(),
                                    function: tools::FunctionCall { name: call.function.name.clone(), arguments: args },
                                };
                                &rewritten
                            };

                            if let Some(refused) = gate_for_ws(app, effective_call) {
                                refused
                            } else if mcp::is_mcp_tool_name(&effective_call.function.name) {
                                crate::dispatch_mcp_tool_call(app, effective_call).await
                            } else {
                                tools::execute_tool_full(effective_call, cwd, persona.as_ref(), &app.undo, &app.db, &app.lsp, client.as_ref(), &app.task_limiter).await
                            }
                        }
                    };
                    result.output = app.plugins.tool_execute_after(&result.name, &result.output, result.success);

                    send_json(socket, &json!({"type": "tool_result", "name": result.name, "success": result.success, "output": result.output})).await?;
                    msgs.push(regin_core::llm::MimirClient::tool_result_message(&result.tool_call_id, &result.output));
                }
            }
            LlmTurn::Text(summary) => {
                send_json(socket, &json!({"type": "done", "summary": summary})).await?;
                return Ok(());
            }
        }
    }
}

/// Parses the plan LLM call's JSON-array response, tolerant of a wrapping
/// markdown code fence (models frequently add one despite being asked not
/// to). Falls back to treating the whole response as a single step rather
/// than failing the goal outright — a plan display is a nice-to-have, not
/// load-bearing for phase 2's execution.
fn parse_plan_steps(text: &str) -> Vec<String> {
    let trimmed = text.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    match serde_json::from_str::<Vec<String>>(trimmed) {
        Ok(steps) if !steps.is_empty() => steps,
        _ if !trimmed.is_empty() => vec![trimmed.to_string()],
        _ => Vec::new(),
    }
}

/// Same WS-scoped `ask`-denial posture as `ws_chat::gate_for_ws` — kept as
/// a separate copy rather than a shared helper since the two call sites
/// diverge slightly in surrounding context and this is the kind of logic
/// that's cheaper to read inline than to chase through an indirection for.
fn gate_for_ws(app: &crate::AppState, call: &tools::ToolCall) -> Option<tools::ToolResult> {
    let tool = call.function.name.clone();
    let command = if tool == "bash" {
        serde_json::from_str::<Value>(&call.function.arguments).ok().and_then(|v| v["command"].as_str().map(str::to_string))
    } else {
        None
    };
    let level = {
        let db = app.db.lock().expect("DB poisoned");
        permission::resolve_permission(&db, &tool, command.as_deref()).unwrap_or(permission::PermissionLevel::Allow)
    };
    match level {
        permission::PermissionLevel::Allow => None,
        permission::PermissionLevel::Deny => {
            Some(tools::ToolResult { tool_call_id: call.id.clone(), name: tool.clone(), output: permission::denied_message(&tool), success: false })
        }
        permission::PermissionLevel::Ask => Some(tools::ToolResult {
            tool_call_id: call.id.clone(),
            name: tool.clone(),
            output: format!("Permission denied for tool {tool}: this tool requires interactive approval, which the web UI goal loop doesn't support yet."),
            success: false,
        }),
    }
}

async fn send_json(socket: &mut WebSocket, value: &Value) -> anyhow::Result<()> {
    socket.send(Message::Text(value.to_string().into())).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plan_steps_handles_a_clean_json_array() {
        let steps = parse_plan_steps(r#"["Read the file", "Edit it", "Run tests"]"#);
        assert_eq!(steps, vec!["Read the file", "Edit it", "Run tests"]);
    }

    #[test]
    fn parse_plan_steps_strips_a_markdown_fence() {
        let steps = parse_plan_steps("```json\n[\"Step one\", \"Step two\"]\n```");
        assert_eq!(steps, vec!["Step one", "Step two"]);
    }

    #[test]
    fn parse_plan_steps_falls_back_to_a_single_step_for_non_json_text() {
        let steps = parse_plan_steps("First read the file, then edit it.");
        assert_eq!(steps, vec!["First read the file, then edit it."]);
    }

    #[test]
    fn parse_plan_steps_returns_empty_for_blank_text() {
        assert_eq!(parse_plan_steps("   "), Vec::<String>::new());
    }
}
