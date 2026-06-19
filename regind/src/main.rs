use anyhow::{anyhow, Context, Result};

use regin_core::{
    config, context, db, desired,
    llm::{LlmTurn, NanoGptClient},
    protocol::{Request, Response},
    reflect, repo, skills,
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

    // FEAT-033: load the desired (to-be) state and surface contradictory targets
    // as problems for a human (fail-safe: malformed files are skipped, logged).
    {
        let states = desired::load_all_desired(
            &config::system_desired_dir(),
            &config::user_desired_dir()?,
        );
        info!("Loaded {} desired-state domain(s)", states.len());
        match desired::check_and_open_problems(&conn, &states) {
            Ok(c) if !c.is_empty() => {
                warn!("Desired-state conflicts opened problems for: {}", c.join(", "));
            }
            Ok(_) => {}
            Err(e) => warn!("Desired-state conflict check failed: {e:#}"),
        }
    }

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

    let refl_state = Arc::clone(&state);
    tokio::spawn(async move { reflection_checker(refl_state).await });

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

/// Per-repo skills (name, content) for the repo resolved from `cwd` (FEAT-009).
fn repo_skills_for(state: &AppState, cwd: Option<&str>) -> Vec<(String, String)> {
    match repo::repo_key(cwd) {
        Some(key) => {
            let db = state.db.lock().expect("DB poisoned");
            db::repo_skill_list(&db, &key).unwrap_or_default()
        }
        None => Vec::new(),
    }
}

/// The per-repo content for one skill name, if the cwd resolves to a repo that
/// has it (FEAT-009).
fn repo_skill_content(state: &AppState, cwd: Option<&str>, name: &str) -> Option<String> {
    let key = repo::repo_key(cwd)?;
    let db = state.db.lock().expect("DB poisoned");
    db::repo_skill_get(&db, &key, name).ok().flatten()
}

/// Build system context messages from the per-repo context + scoped memories
/// (FEAT-008). The repo is identified by its filesystem path; a legacy in-repo
/// `.repo/regin/context.md` is imported into the store once.
fn build_context(state: &AppState, cwd: Option<&str>) -> Vec<Value> {
    let key = repo::repo_key(cwd);
    let (memories, repo_ctx) = {
        let db = state.db.lock().expect("DB poisoned");
        if let Some(k) = &key {
            // one-time legacy import
            if db::repo_context_get(&db, k).ok().flatten().is_none() {
                if let Some(legacy) = repo::read_legacy_context(cwd) {
                    let _ = db::repo_context_set(&db, k, &legacy);
                    info!(repo = %k, "imported legacy .repo/regin/context.md into the store");
                }
            }
        }
        let mems = db::memory_list_for_repo(&db, key.as_deref()).unwrap_or_default();
        let rc = key.as_deref().and_then(|k| db::repo_context_get(&db, k).ok().flatten());
        (mems, rc)
    };
    let mut msgs = Vec::new();
    if let Some(system) = context::build_system_prompt(repo_ctx.as_deref(), &memories) {
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
    // FEAT-011: a configured persona scopes the tool ceiling + shapes the prompt.
    let persona = regin_core::persona::Persona::from_env().unwrap_or(None);
    let tool_defs = tools::tool_definitions_for(persona.as_ref());

    // Build full message list: persona preamble + system context + user conversation
    let mut msgs: Vec<Value> = Vec::new();
    if let Some(p) = &persona {
        if !p.prompt.is_empty() {
            msgs.push(serde_json::json!({ "role": "system", "content": p.prompt }));
        }
    }
    msgs.extend(build_context(state, cwd));
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

                    let result = tools::execute_tool_gated(call, cwd, persona.as_ref()).await;

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

        Request::SkillList { cwd } => {
            let sys = config::system_skills_dir();
            let usr = config::user_skills_dir()?;
            let repo_skills = repo_skills_for(state, cwd.as_deref());
            let all = skills::list_all_skills_scoped(&sys, &usr, &repo_skills)?;
            let infos = all.iter().map(|s| regin_core::types::SkillInfo {
                name: s.name.clone(), description: s.description.clone(), source: s.source.to_string(),
            }).collect();
            send(w, &Response::SkillList { skills: infos }).await?;
        }

        Request::SkillShow { name, cwd } => {
            let repo_content = repo_skill_content(state, cwd.as_deref(), &name);
            let skill = skills::load_skill_scoped(
                &config::system_skills_dir(), &config::user_skills_dir()?, repo_content.as_deref(), &name,
            )?;
            let files = skill.files.iter().map(|(f, _)| f.clone()).collect();
            send(w, &Response::SkillDetail {
                name: skill.name, description: skill.description, prompt: skill.prompt, files,
            }).await?;
        }

        Request::TaskExec { skill: name, cwd } => {
            let repo_content = repo_skill_content(state, cwd.as_deref(), &name);
            let skill = skills::load_skill_scoped(
                &config::system_skills_dir(), &config::user_skills_dir()?, repo_content.as_deref(), &name,
            )?;
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

        // --- Hermes reflection (FEAT-006) ---
        Request::MemoryReflect => {
            let stats = run_reflection(state).await?;
            send(w, &Response::ReflectStats {
                episodes: stats.episodes as u32,
                reinforced: stats.reinforced as u32,
                created: stats.created as u32,
                decayed: stats.decayed as u32,
            }).await?;
        }

        // --- ITIL: Incidents ---
        Request::IncidentOpen { title, description, severity } => {
            let inc = { let db = state.db.lock().expect("DB poisoned");
                db::incident_open(&db, &title, &description, &severity, "manual", None)? };
            send(w, &Response::Ok { message: format!("Incident opened: {} [{}]", inc.id, inc.severity) }).await?;
        }
        Request::IncidentList { status } => {
            let incidents = { let db = state.db.lock().expect("DB poisoned"); db::incident_list(&db, status.as_deref())? };
            send(w, &Response::Incidents { incidents }).await?;
        }
        Request::IncidentShow { id } => {
            let inc = { let db = state.db.lock().expect("DB poisoned"); db::incident_get(&db, &id)? };
            match inc {
                Some(i) => send(w, &Response::Incidents { incidents: vec![i] }).await?,
                None => send(w, &Response::Error { message: format!("No incident {id}") }).await?,
            }
        }
        Request::IncidentUpdate { id, status } => {
            { let db = state.db.lock().expect("DB poisoned"); db::incident_set_status(&db, &id, &status)?; }
            send(w, &Response::Ok { message: format!("Incident {id} -> {status}") }).await?;
        }
        Request::IncidentResolve { id, resolution } => {
            { let db = state.db.lock().expect("DB poisoned"); db::incident_resolve(&db, &id, &resolution)?; }
            send(w, &Response::Ok { message: format!("Incident {id} resolved") }).await?;
        }
        Request::IncidentClose { id } => {
            { let db = state.db.lock().expect("DB poisoned"); db::incident_close(&db, &id)?; }
            send(w, &Response::Ok { message: format!("Incident {id} closed") }).await?;
        }
        Request::IncidentBlock { id, workaround } => {
            { let db = state.db.lock().expect("DB poisoned"); db::incident_block(&db, &id, &workaround)?; }
            send(w, &Response::Ok { message: format!("Incident {id} blocked (workaround recorded)") }).await?;
        }

        // --- ITIL: Changes ---
        Request::ChangeRecord { title, description, incident_id, problem_id, before, after } => {
            let c = { let db = state.db.lock().expect("DB poisoned");
                db::change_record(&db, &title, &description, incident_id.as_deref(), problem_id.as_deref(), before.as_deref(), after.as_deref())? };
            send(w, &Response::Ok { message: format!("Change recorded: {}", c.id) }).await?;
        }
        Request::ChangeRequestApproval { id } => {
            { let db = state.db.lock().expect("DB poisoned"); db::change_request_approval(&db, &id)?; }
            send(w, &Response::Ok { message: format!("Change {id} -> pending_approval") }).await?;
        }
        Request::ChangeApprove { id, approved_by } => {
            { let db = state.db.lock().expect("DB poisoned"); db::change_approve(&db, &id, &approved_by)?; }
            send(w, &Response::Ok { message: format!("Change {id} approved by {approved_by}") }).await?;
        }
        Request::ChangeList => {
            let changes = { let db = state.db.lock().expect("DB poisoned"); db::change_list(&db)? };
            send(w, &Response::Changes { changes }).await?;
        }
        Request::ChangeShow { id } => {
            let c = { let db = state.db.lock().expect("DB poisoned"); db::change_get(&db, &id)? };
            match c {
                Some(c) => send(w, &Response::Changes { changes: vec![c] }).await?,
                None => send(w, &Response::Error { message: format!("No change {id}") }).await?,
            }
        }
        Request::ChangeApply { id } => {
            { let db = state.db.lock().expect("DB poisoned"); db::change_apply(&db, &id)?; }
            send(w, &Response::Ok { message: format!("Change {id} applied") }).await?;
        }
        Request::ChangeClose { id } => {
            { let db = state.db.lock().expect("DB poisoned"); db::change_close(&db, &id)?; }
            send(w, &Response::Ok { message: format!("Change {id} closed") }).await?;
        }

        // --- ITIL: Problems ---
        Request::ProblemOpen { title, description } => {
            let p = { let db = state.db.lock().expect("DB poisoned"); db::problem_open(&db, &title, &description)? };
            send(w, &Response::Ok { message: format!("Problem opened: {}", p.id) }).await?;
        }
        Request::ProblemList { status } => {
            let problems = { let db = state.db.lock().expect("DB poisoned"); db::problem_list(&db, status.as_deref())? };
            send(w, &Response::Problems { problems }).await?;
        }
        Request::ProblemShow { id } => {
            let p = { let db = state.db.lock().expect("DB poisoned"); db::problem_get(&db, &id)? };
            match p {
                Some(p) => send(w, &Response::Problems { problems: vec![p] }).await?,
                None => send(w, &Response::Error { message: format!("No problem {id}") }).await?,
            }
        }
        Request::ProblemLink { problem_id, incident_id } => {
            { let db = state.db.lock().expect("DB poisoned"); db::link_incident_to_problem(&db, &problem_id, &incident_id)?; }
            send(w, &Response::Ok { message: format!("Linked incident {incident_id} to problem {problem_id}") }).await?;
        }
        Request::ProblemKnownError { id, root_cause } => {
            { let db = state.db.lock().expect("DB poisoned"); db::problem_set_known_error(&db, &id, &root_cause)?; }
            send(w, &Response::Ok { message: format!("Problem {id} -> known_error") }).await?;
        }
        Request::ProblemClose { id } => {
            { let db = state.db.lock().expect("DB poisoned"); db::problem_close(&db, &id)?; }
            send(w, &Response::Ok { message: format!("Problem {id} closed") }).await?;
        }
        Request::ProblemHypothesisAdd { problem_id, text } => {
            let h = { let db = state.db.lock().expect("DB poisoned"); db::hypothesis_add(&db, &problem_id, &text)? };
            send(w, &Response::Ok { message: format!("Hypothesis added: {}", h.id) }).await?;
        }
        Request::ProblemHypothesisList { problem_id } => {
            let hypotheses = { let db = state.db.lock().expect("DB poisoned"); db::hypothesis_list(&db, &problem_id)? };
            send(w, &Response::Hypotheses { hypotheses }).await?;
        }
        Request::ProblemHypothesisStatus { id, status } => {
            { let db = state.db.lock().expect("DB poisoned"); db::hypothesis_set_status(&db, &id, &status)?; }
            send(w, &Response::Ok { message: format!("Hypothesis {id} -> {status}") }).await?;
        }

        // --- Desired state (to-be) — FEAT-033 ---
        Request::DesiredList => {
            let states = desired::load_all_desired(&config::system_desired_dir(), &config::user_desired_dir()?);
            send(w, &Response::DesiredListResp { items: desired::summaries(&states) }).await?;
        }
        Request::DesiredShow { domain } => {
            let ds = desired::load_desired(&config::system_desired_dir(), &config::user_desired_dir()?, &domain)?;
            match ds {
                Some(state) => send(w, &Response::DesiredDetail { state: Box::new(state) }).await?,
                None => send(w, &Response::Error { message: format!("No desired state for domain `{domain}`") }).await?,
            }
        }
        Request::DesiredCheck => {
            let states = desired::load_all_desired(&config::system_desired_dir(), &config::user_desired_dir()?);
            let conflicted = { let db = state.db.lock().expect("DB poisoned"); desired::check_and_open_problems(&db, &states)? };
            let message = if conflicted.is_empty() {
                format!("Checked {} desired-state domain(s); no conflicts.", states.len())
            } else {
                format!("Conflicts in {} domain(s): {} — problem(s) opened for human review.", conflicted.len(), conflicted.join(", "))
            };
            send(w, &Response::Ok { message }).await?;
        }

        // --- Skill authoring (FEAT-007 / FEAT-009) ---
        Request::TaskCreate { name, from_prompt, force, repo, cwd } => {
            let content = match &from_prompt {
                Some(goal) => {
                    let client = state.llm_client()?; // needs an API key
                    let prompt = format!(
                        "Write a regin skill file (skill.md) for an operational task named `{name}`.\n\
                         Goal: {goal}\n\n\
                         Output ONLY the file content, no code fences. The FIRST line must be a\n\
                         concise one-line description (shown as the skill description). Then a blank\n\
                         line, then clear step-by-step instructions for the agent, which has tools:\n\
                         bash, file read/write/edit, and web search."
                    );
                    client.chat_completion(&[ChatMessage::user(prompt)]).await?
                }
                None => skills::skill_template(&name),
            };
            if repo {
                // FEAT-009: store in the per-repo store keyed by repo path.
                match repo::repo_key(cwd.as_deref()) {
                    Some(key) => {
                        let existed = { let db = state.db.lock().expect("DB poisoned"); db::repo_skill_get(&db, &key, &name)?.is_some() };
                        if existed && !force {
                            send(w, &Response::Error { message: format!("Repo skill '{name}' already exists (use --force)") }).await?;
                        } else {
                            { let db = state.db.lock().expect("DB poisoned"); db::repo_skill_save(&db, &key, &name, &content)?; }
                            send(w, &Response::SkillCreated { path: format!("[repo store] {key} :: {name}"), shadows_system: false }).await?;
                        }
                    }
                    None => send(w, &Response::Error { message: "No repo resolved for --repo".into() }).await?,
                }
            } else {
                let user_dir = config::user_skills_dir()?;
                let path = skills::create_skill(&user_dir, &name, &content, force)?;
                let shadows = skills::system_skill_exists(&config::system_skills_dir(), &name);
                send(w, &Response::SkillCreated { path: path.display().to_string(), shadows_system: shadows }).await?;
            }
        }

        // --- Per-repo context (FEAT-008) ---
        Request::ContextShow { cwd } => {
            let key = repo::repo_key(cwd.as_deref());
            let content = match &key {
                Some(k) => { let db = state.db.lock().expect("DB poisoned"); db::repo_context_get(&db, k)? }
                None => None,
            };
            send(w, &Response::Context { repo_key: key, content }).await?;
        }
        Request::ContextSet { cwd, content } => {
            match repo::repo_key(cwd.as_deref()) {
                Some(k) => {
                    { let db = state.db.lock().expect("DB poisoned"); db::repo_context_set(&db, &k, &content)?; }
                    send(w, &Response::Ok { message: format!("Repo context set for {k}") }).await?;
                }
                None => send(w, &Response::Error { message: "No working directory to key the repo".into() }).await?,
            }
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
            {
                let db = state.db.lock().expect("DB poisoned");
                let _ = db::save_task_run(&db, &sched.skill, &status, &output, &started_at, &finished_at);
                // FEAT-004: evaluate the result; gated by monitor.auto_incident.
                // Fails safe — a bad evaluation never breaks the scheduler loop.
                let auto = db::setting_get(&db, "monitor.auto_incident").map(|v| v == "true").unwrap_or(false);
                if auto {
                    let severity = db::setting_get(&db, "monitor.severity").unwrap_or_else(|_| "medium".into());
                    let threshold = db::setting_get(&db, "monitor.recurrence_threshold")
                        .ok()
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(3);
                    match db::monitor_evaluate(&db, &sched.skill, &status, &output, &severity, threshold) {
                        Ok(o) => {
                            if o.created_incident {
                                info!(skill = %sched.skill, incident = ?o.incident_id, "monitor opened incident");
                            }
                            if let Some(p) = &o.problem_id {
                                warn!(skill = %sched.skill, problem = %p, "monitor: recurrence -> problem");
                            }
                        }
                        Err(e) => error!(skill = %sched.skill, "monitor_evaluate: {e}"),
                    }
                }
            }
            let last_run = chrono::Utc::now().to_rfc3339();
            if let Ok(next) = compute_next_run(&sched.interval) {
                let db = state.db.lock().expect("DB poisoned");
                let _ = db::update_schedule_after_run(&db, &sched.skill, &last_run, &next);
            }
        }
    }
}

/// Run one Hermes reflection pass (FEAT-006). The DB lock is released around the
/// network call so the daemon stays responsive.
async fn run_reflection(state: &Arc<AppState>) -> Result<reflect::ReflectionStats> {
    let client = state.llm_client()?;
    let window = 100usize;
    let (episodes, existing, decay_before) = {
        let db = state.db.lock().expect("DB poisoned");
        let decay_days = db::setting_get(&db, "memory.decay_days")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(30);
        let decay_before = (chrono::Utc::now() - chrono::Duration::days(decay_days)).to_rfc3339();
        let (e, ex) = reflect::gather(&db, window)?;
        (e, ex, decay_before)
    };
    if episodes.is_empty() {
        let db = state.db.lock().expect("DB poisoned");
        let decayed = db::memory_decay(&db, &decay_before)?;
        return Ok(reflect::ReflectionStats { decayed, ..Default::default() });
    }
    let prompt = reflect::reflection_prompt(&episodes, &existing);
    let text = client.chat_completion(&[ChatMessage::user(prompt)]).await?;
    let proposals = reflect::parse_proposals(&text)?;
    let db = state.db.lock().expect("DB poisoned");
    reflect::apply(&db, &episodes, &proposals, &decay_before)
}

/// Periodically reflect episodes into semantic memory, on memory.reflect_interval.
/// Fails safe — errors are logged and never stop the loop.
async fn reflection_checker(state: Arc<AppState>) {
    loop {
        let interval_str = {
            let db = state.db.lock().expect("DB poisoned");
            db::setting_get(&db, "memory.reflect_interval").unwrap_or_else(|_| "daily".into())
        };
        let dur = parse_interval(&interval_str)
            .ok()
            .and_then(|d| d.to_std().ok())
            .unwrap_or_else(|| std::time::Duration::from_secs(86_400));
        tokio::time::sleep(dur).await;

        match run_reflection(&state).await {
            Ok(s) if s.episodes > 0 || s.decayed > 0 => info!(
                episodes = s.episodes,
                reinforced = s.reinforced,
                created = s.created,
                decayed = s.decayed,
                "reflection pass"
            ),
            Ok(_) => {}
            Err(e) => warn!("reflection: {e}"),
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
