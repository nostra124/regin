use anyhow::{anyhow, Context, Result};

use regin_core::{
    config, context, db,
    llm::{LlmTurn, NanoGptClient},
    protocol::{Request, Response},
    skills,
    tools,
    types::ChatMessage,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::signal;
use tracing::{error, info, warn};

struct AppState {
    db: Mutex<rusqlite::Connection>,
}

unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

impl AppState {
    fn llm_client(&self) -> Result<NanoGptClient> {
        let db = self.db.lock().expect("DB poisoned");
        let base_url = db::setting_get(&db, "nanogpt.base_url")?;
        let api_key = db::setting_get(&db, "nanogpt.api_key")?;
        let model = db::setting_get(&db, "nanogpt.model")?;
        if api_key.is_empty() {
            return Err(anyhow!("nanogpt.api_key not set. Run: regin config set nanogpt.api_key <key>"));
        }
        Ok(NanoGptClient::new(base_url, api_key, model))
    }

    fn load_memories(&self) -> Vec<regin_core::types::Memory> {
        let db = self.db.lock().expect("DB poisoned");
        db::memory_list(&db, None).unwrap_or_default()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("regind starting up");

    let db_path = config::db_path()?;
    let conn = db::init_db(&db_path)?;
    info!("Database: {}", db_path.display());

    let socket_path = config::socket_path()?;
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to bind {}", socket_path.display()))?;
    info!("Listening on {}", socket_path.display());

    let state = Arc::new(AppState { db: Mutex::new(conn) });

    let sched_state = Arc::clone(&state);
    tokio::spawn(async move { schedule_checker(sched_state).await });

    let cleanup = socket_path.clone();
    let result = tokio::select! {
        res = accept_loop(listener, state) => res,
        _ = shutdown_signal() => { info!("Shutdown"); Ok(()) }
    };
    let _ = std::fs::remove_file(&cleanup);
    info!("regind stopped");
    result
}

async fn accept_loop(listener: UnixListener, state: Arc<AppState>) -> Result<()> {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let s = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, s).await {
                        error!("Connection error: {e}");
                    }
                });
            }
            Err(e) => error!("Accept error: {e}"),
        }
    }
}

async fn handle_connection(stream: tokio::net::UnixStream, state: Arc<AppState>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut buf = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        if buf.read_line(&mut line).await? == 0 { break; }
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let request: Request = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => { send(&mut writer, &Response::Error { message: format!("Bad request: {e}") }).await?; continue; }
        };
        if let Err(e) = dispatch(request, &state, &mut writer).await {
            let _ = send(&mut writer, &Response::Error { message: format!("{e}") }).await;
        }
    }
    Ok(())
}

async fn send(w: &mut tokio::net::unix::OwnedWriteHalf, r: &Response) -> Result<()> {
    let mut json = serde_json::to_string(r)?;
    json.push('\n');
    w.write_all(json.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}

/// Build system context messages from context files + memories.
fn build_context(state: &AppState, cwd: Option<&str>) -> Vec<Value> {
    let memories = state.load_memories();
    let mut msgs = Vec::new();
    if let Some(system) = context::build_system_prompt(cwd, &memories) {
        msgs.push(serde_json::json!({ "role": "system", "content": system }));
    }
    msgs
}

/// The agentic loop: call LLM with tools, execute tool calls, repeat until text.
/// Streams text chunks and tool activity to the client.
async fn agentic_chat(
    state: &Arc<AppState>,
    _conversation_id: &str,
    user_messages: &[ChatMessage],
    cwd: Option<&str>,
    w: &mut tokio::net::unix::OwnedWriteHalf,
) -> Result<String> {
    let client = state.llm_client()?;
    let tool_defs = tools::tool_definitions();

    // Build full message list: system context + user conversation
    let mut msgs: Vec<Value> = build_context(state, cwd);
    for m in user_messages {
        msgs.push(NanoGptClient::msg_to_value(m));
    }

    // Agentic loop
    loop {
        let turn = client.chat_turn(&msgs, Some(&tool_defs)).await?;
        match turn {
            LlmTurn::ToolCalls { assistant_message, calls } => {
                // Add assistant message to conversation
                msgs.push(assistant_message);

                // Execute each tool call
                for call in &calls {
                    send(w, &Response::ToolCallEvent {
                        name: call.function.name.clone(),
                        arguments: call.function.arguments.clone(),
                    }).await?;

                    let result = tools::execute_tool(call, cwd).await;

                    send(w, &Response::ToolResultEvent {
                        name: result.name.clone(),
                        success: result.success,
                        output: result.output.clone(),
                    }).await?;

                    // Add tool result to conversation
                    msgs.push(NanoGptClient::tool_result_message(&result.tool_call_id, &result.output));
                }
                // Continue the loop — call LLM again with tool results
            }
            LlmTurn::Text(text) => {
                // Final text response — stream it to the client
                // For the final response, re-request as streaming for nice UX
                // But we already have the text, so just send it as chunks
                if !text.is_empty() {
                    // Send as a single chunk for simplicity
                    send(w, &Response::StreamChunk { token: text.clone() }).await?;
                }
                return Ok(text);
            }
        }
    }
}

async fn dispatch(
    req: Request,
    state: &Arc<AppState>,
    w: &mut tokio::net::unix::OwnedWriteHalf,
) -> Result<()> {
    match req {
        Request::Ping => send(w, &Response::Pong).await?,

        Request::SkillList => {
            let sys = config::system_skills_dir();
            let usr = config::user_skills_dir()?;
            let all = skills::list_all_skills(&sys, &usr)?;
            let infos = all.iter().map(|s| regin_core::types::SkillInfo {
                name: s.name.clone(), description: s.description.clone(), source: s.source.to_string(),
            }).collect();
            send(w, &Response::SkillList { skills: infos }).await?;
        }

        Request::SkillShow { name } => {
            let skill = skills::load_skill(&config::system_skills_dir(), &config::user_skills_dir()?, &name)?;
            let files = skill.files.iter().map(|(f, _)| f.clone()).collect();
            send(w, &Response::SkillDetail {
                name: skill.name, description: skill.description, prompt: skill.prompt, files,
            }).await?;
        }

        Request::TaskExec { skill: name, cwd } => {
            let skill = skills::load_skill(&config::system_skills_dir(), &config::user_skills_dir()?, &name)?;
            let run = exec_skill_agentic(&skill, state, cwd.as_deref(), w).await?;
            send(w, &Response::TaskResult { run }).await?;
        }

        Request::TaskSchedule { skill, interval } => {
            let _ = skills::load_skill(&config::system_skills_dir(), &config::user_skills_dir()?, &skill)?;
            let next_run = compute_next_run(&interval)?;
            { let db = state.db.lock().expect("DB poisoned"); db::save_schedule(&db, &skill, &interval, &next_run)?; }
            send(w, &Response::Ok { message: format!("'{skill}' scheduled {interval} (next: {next_run})") }).await?;
        }

        Request::TaskUnschedule { skill } => {
            { let db = state.db.lock().expect("DB poisoned"); db::delete_schedule(&db, &skill)?; }
            send(w, &Response::Ok { message: format!("Schedule removed for '{skill}'") }).await?;
        }

        Request::TaskSchedules => {
            let s = { let db = state.db.lock().expect("DB poisoned"); db::list_schedules(&db)? };
            send(w, &Response::SchedulesList { schedules: s }).await?;
        }

        Request::RunsList { skill, limit } => {
            let runs = {
                let db = state.db.lock().expect("DB poisoned");
                match skill { Some(ref n) => db::get_task_runs(&db, n, limit as usize)?, None => db::get_all_task_runs(&db, limit as usize)? }
            };
            send(w, &Response::RunsList { runs }).await?;
        }

        Request::ChatSend { conversation_id, messages, cwd } => {
            let title = messages.iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.chars().take(80).collect::<String>())
                .unwrap_or_else(|| "Untitled".into());

            if let Some(user_msg) = messages.iter().rev().find(|m| m.role == "user") {
                let db = state.db.lock().expect("DB poisoned");
                db::save_message(&db, &conversation_id, &title, "user", &user_msg.content)?;
            }

            let full = agentic_chat(state, &conversation_id, &messages, cwd.as_deref(), w).await?;

            {
                let db = state.db.lock().expect("DB poisoned");
                db::save_message(&db, &conversation_id, &title, "assistant", &full)?;
            }
            send(w, &Response::StreamDone { conversation_id }).await?;
        }

        Request::ChatNew => {
            let id = uuid::Uuid::new_v4().to_string();
            send(w, &Response::ChatNew { conversation_id: id }).await?;
        }

        Request::ChatHistory => {
            let c = { let db = state.db.lock().expect("DB poisoned"); db::list_conversations(&db)? };
            send(w, &Response::ChatHistory { conversations: c }).await?;
        }

        Request::ConfigList => {
            let e = { let db = state.db.lock().expect("DB poisoned"); db::setting_list(&db)? };
            send(w, &Response::ConfigEntries { entries: e }).await?;
        }

        Request::ConfigGet { key } => {
            let v = { let db = state.db.lock().expect("DB poisoned"); db::setting_get(&db, &key)? };
            send(w, &Response::ConfigValue { key, value: v }).await?;
        }

        Request::ConfigSet { key, value } => {
            { let db = state.db.lock().expect("DB poisoned"); db::setting_set(&db, &key, &value)?; }
            send(w, &Response::Ok { message: format!("{key} = {value}") }).await?;
        }

        Request::MemoryList { category } => {
            let mems = { let db = state.db.lock().expect("DB poisoned"); db::memory_list(&db, category.as_deref())? };
            send(w, &Response::MemoryList { memories: mems }).await?;
        }

        Request::MemorySearch { query } => {
            let mems = { let db = state.db.lock().expect("DB poisoned"); db::memory_search(&db, &query)? };
            send(w, &Response::MemoryList { memories: mems }).await?;
        }

        Request::MemorySave { category, content } => {
            let m = { let db = state.db.lock().expect("DB poisoned"); db::memory_save(&db, &category, &content)? };
            send(w, &Response::Ok { message: format!("Memory saved: {} [{}]", m.id, m.category) }).await?;
        }

        Request::MemoryUpdate { id, content } => {
            { let db = state.db.lock().expect("DB poisoned"); db::memory_update(&db, &id, &content)?; }
            send(w, &Response::Ok { message: format!("Memory {id} updated") }).await?;
        }

        Request::MemoryDelete { id } => {
            { let db = state.db.lock().expect("DB poisoned"); db::memory_delete(&db, &id)?; }
            send(w, &Response::Ok { message: format!("Memory {id} deleted") }).await?;
        }
    }
    Ok(())
}

/// Run a skill through the agentic loop (with tools).
async fn exec_skill_agentic(
    skill: &skills::Skill,
    state: &Arc<AppState>,
    cwd: Option<&str>,
    w: &mut tokio::net::unix::OwnedWriteHalf,
) -> Result<regin_core::types::TaskRun> {
    info!(skill = %skill.name, "Running skill (agentic)");
    let started_at = chrono::Utc::now().to_rfc3339();

    let mut content = skill.prompt.clone();
    if !skill.files.is_empty() {
        content.push_str("\n\n--- Supporting Files ---\n");
        for (fname, body) in &skill.files {
            content.push_str(&format!("\n### {fname}\n```\n{body}\n```\n"));
        }
    }

    let messages = vec![ChatMessage::user(content)];
    let result = agentic_chat(state, "_task", &messages, cwd, w).await;

    let (status, output) = match result {
        Ok(text) => ("success".to_string(), text),
        Err(e) => {
            error!(skill = %skill.name, error = %e, "Skill failed");
            ("error".to_string(), format!("{e}"))
        }
    };

    let finished_at = chrono::Utc::now().to_rfc3339();
    let db = state.db.lock().expect("DB poisoned");
    let run = db::save_task_run(&db, &skill.name, &status, &output, &started_at, &finished_at)?;
    Ok(run)
}

async fn schedule_checker(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        let now = chrono::Utc::now().to_rfc3339();
        let due = {
            let db = state.db.lock().expect("DB poisoned");
            match db::get_due_schedules(&db, &now) { Ok(s) => s, Err(e) => { error!("Schedule check: {e}"); continue; } }
        };
        for sched in due {
            info!(skill = %sched.skill, "Scheduled task");
            let sys = config::system_skills_dir();
            let usr = match config::user_skills_dir() { Ok(d) => d, Err(e) => { error!("{e}"); continue; } };
            let skill = match skills::load_skill(&sys, &usr, &sched.skill) { Ok(s) => s, Err(e) => { error!("Load: {e}"); continue; } };
            let client = match state.llm_client() { Ok(c) => c, Err(e) => { warn!("{e}"); continue; } };
            // Scheduled tasks use simple non-agentic execution (no writer to stream to)
            let started_at = chrono::Utc::now().to_rfc3339();
            let mut content = skill.prompt.clone();
            if !skill.files.is_empty() {
                content.push_str("\n\n--- Supporting Files ---\n");
                for (f, b) in &skill.files { content.push_str(&format!("\n### {f}\n```\n{b}\n```\n")); }
            }
            let msgs = vec![ChatMessage::user(content)];
            let (status, output) = match client.chat_completion(&msgs).await {
                Ok(r) => ("success".to_string(), r),
                Err(e) => { error!(skill = %sched.skill, "Run: {e}"); ("error".to_string(), format!("{e}")) }
            };
            let finished_at = chrono::Utc::now().to_rfc3339();
            { let db = state.db.lock().expect("DB poisoned"); let _ = db::save_task_run(&db, &sched.skill, &status, &output, &started_at, &finished_at); }
            let last_run = chrono::Utc::now().to_rfc3339();
            if let Ok(next) = compute_next_run(&sched.interval) {
                let db = state.db.lock().expect("DB poisoned");
                let _ = db::update_schedule_after_run(&db, &sched.skill, &last_run, &next);
            }
        }
    }
}

fn parse_interval(interval: &str) -> Result<chrono::Duration> {
    match interval {
        "hourly" => Ok(chrono::Duration::hours(1)),
        "daily" => Ok(chrono::Duration::days(1)),
        "weekly" => Ok(chrono::Duration::weeks(1)),
        "monthly" => Ok(chrono::Duration::days(30)),
        s if s.starts_with("every ") => {
            let spec = &s[6..];
            let unit = spec.chars().last().ok_or_else(|| anyhow!("Empty interval"))?;
            let num: i64 = spec[..spec.len() - 1].parse().context("Bad number")?;
            match unit {
                's' => Ok(chrono::Duration::seconds(num)),
                'm' => Ok(chrono::Duration::minutes(num)),
                'h' => Ok(chrono::Duration::hours(num)),
                'd' => Ok(chrono::Duration::days(num)),
                _ => Err(anyhow!("Unknown unit: {unit}")),
            }
        }
        _ => Err(anyhow!("Unknown interval: {interval}")),
    }
}

fn compute_next_run(interval: &str) -> Result<String> {
    Ok((chrono::Utc::now() + parse_interval(interval)?).to_rfc3339())
}

async fn shutdown_signal() {
    let ctrl_c = signal::ctrl_c();
    #[cfg(unix)]
    let mut term = signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM");
    tokio::select! { _ = ctrl_c => {} _ = term.recv() => {} }
}
