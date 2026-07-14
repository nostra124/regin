//! Chat WebSocket (FEAT-087, acceptance criterion 7): reuses the *exact*
//! same tool-execution stack as `regin`'s CLI chat (`execute_tool_full`,
//! plugin hooks, MCP dispatch) — only the transport differs (JSON WS
//! frames instead of newline-delimited `protocol::Response`).
//!
//! One scope decision: FEAT-080's `ask`-level permission gate needs a
//! synchronous rendezvous with a human answering on a *separate*
//! connection (the CLI's `Request::PermissionResponse`), which has no
//! analogue over a single WS connection yet. Rather than build that
//! (a separate ticket's worth of work), an `ask`-level tool over this
//! endpoint is treated as denied, with an explanation — the same
//! fail-safe-not-fail-open posture `gate_tool_call` documents, just
//! without the interactive round-trip.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use serde_json::{json, Value};
use std::sync::Arc;

use super::{AuthedUser, SharedState, WebuiState};
use regin_core::{llm::LlmTurn, mcp, permission, persona::Persona, plugin, tools, types::ChatMessage};

pub async fn handler(ws: WebSocketUpgrade, State(state): SharedState, _user: AuthedUser) -> impl IntoResponse {
    ws.on_upgrade(move |socket| run(socket, state))
}

#[derive(serde::Deserialize)]
struct IncomingTurn {
    message: String,
    #[serde(default)]
    cwd: Option<String>,
}

async fn run(mut socket: WebSocket, state: Arc<WebuiState>) {
    let mut history: Vec<ChatMessage> = Vec::new();
    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Text(text) = msg else { continue };
        let turn: IncomingTurn = match serde_json::from_str(&text) {
            Ok(t) => t,
            Err(e) => {
                let _ = send_json(&mut socket, &json!({"type": "error", "message": format!("invalid message: {e}")})).await;
                continue;
            }
        };
        history.push(ChatMessage::user(turn.message));
        match run_turn(&state, &history, turn.cwd.as_deref(), &mut socket).await {
            Ok(reply) => history.push(ChatMessage { role: "assistant".to_string(), content: reply }),
            Err(e) => {
                let _ = send_json(&mut socket, &json!({"type": "error", "message": format!("{e:#}")})).await;
            }
        }
    }
}

/// Mirrors `main::agentic_chat`'s loop, emitting JSON WS events instead of
/// `protocol::Response` frames on a socket-transport `AsyncWrite`.
async fn run_turn(state: &Arc<WebuiState>, user_messages: &[ChatMessage], cwd: Option<&str>, socket: &mut WebSocket) -> anyhow::Result<String> {
    let app = &state.app;
    let client = app.llm_client()?;
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
    for m in user_messages {
        msgs.push(regin_core::llm::MimirClient::msg_to_value(m));
    }

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
            LlmTurn::Text(text) => {
                send_json(socket, &json!({"type": "done", "text": text})).await?;
                return Ok(text);
            }
        }
    }
}

/// `ask`-level tools are denied over WS (see module doc comment); `allow`
/// proceeds, `deny` refuses — same as `main::gate_tool_call`'s non-`ask`
/// branches.
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
            output: format!("Permission denied for tool {tool}: this tool requires interactive approval, which the web UI chat doesn't support yet."),
            success: false,
        }),
    }
}

async fn send_json(socket: &mut WebSocket, value: &Value) -> anyhow::Result<()> {
    socket.send(Message::Text(value.to_string().into())).await?;
    Ok(())
}
