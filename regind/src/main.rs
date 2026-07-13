use anyhow::{anyhow, Context, Result};

use regin_core::{
    audit, bus, config, context, db, desired, filters, goal, identity_db, kpi,
    llm::{LlmClient, LlmTurn, MimirClient},
    lsp,
    objective,
    opskill,
    greeting,
    mode,
    posture,
    promotion,
    protocol::{Request, Response},
    push,
    reflect, repo, schedule, skills, soul,
    subagent,
    tools,
    types::ChatMessage,
    undo,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::signal;
use tracing::{error, info, warn};

struct AppState {
    db: Mutex<rusqlite::Connection>,
    identity_db: Mutex<rusqlite::Connection>,
    /// Test-only injection seam (FEAT-071): when set, `llm_client()` returns
    /// this instead of constructing a `MimirClient` from live config. `None`
    /// in production.
    llm_override: Option<Arc<dyn LlmClient>>,
    /// Ephemeral edit history for the `undo`/`undo_list` tools (FEAT-085).
    /// In-memory only, by design — lost on daemon restart.
    undo: Mutex<undo::UndoStore>,
    /// Spawned language servers + debounce state for the `diagnostics` tool
    /// and automatic post-edit diagnostics (FEAT-078). Always constructed;
    /// nothing is spawned until `lsp.enabled` is set (`lsp::plan_diagnostics`
    /// checks it on every call).
    lsp: lsp::LspContext,
    /// Bounds concurrent subagents spawned via the `task` tool (FEAT-079,
    /// `task.max_concurrency`). Sized once at construction from that
    /// setting's value — see `subagent::TaskLimiter`'s doc comment.
    task_limiter: subagent::TaskLimiter,
}

unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

impl AppState {
    /// The single seam through which the daemon obtains LLM completions.
    /// Reads `mimir.*` settings fresh on every call (not cached on
    /// `AppState`) so `regin config set mimir.*` takes effect without a
    /// daemon restart — the injected override (tests) bypasses that read
    /// entirely.
    fn llm_client(&self) -> Result<Arc<dyn LlmClient>> {
        if let Some(over) = &self.llm_override {
            return Ok(over.clone());
        }
        let db = self.db.lock().expect("DB poisoned");
        let base_url = db::setting_get(&db, "mimir.base_url")?;
        let fingerprint = db::setting_get(&db, "mimir.fingerprint")?;
        let model = db::setting_get(&db, "mimir.model")?;
        if fingerprint.is_empty() {
            return Err(anyhow!(
                "mimir.fingerprint not set. Regin reaches its LLM through Mimir — set the \
                 approved access credential: regin config set mimir.fingerprint <fingerprint>"
            ));
        }
        Ok(Arc::new(MimirClient::new(base_url, fingerprint, model)))
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

    let identity_path = config::identity_db_path()?;
    let identity_conn = identity_db::init_identity_db(&identity_path)?;
    info!("Identity database: {}", identity_path.display());

    // FEAT-022: one-shot migration of episodes + memories from regin.db → identity.db.
    match identity_db::migrate_legacy(&conn, &identity_conn) {
        Ok(r) if r.did_run => info!("Legacy migration: {} episodes, {} memories", r.episodes, r.memories),
        Ok(_) => info!("Legacy migration already complete"),
        Err(e) => warn!("Legacy migration failed (non-fatal): {e:#}"),
    }

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

    let task_max_concurrency: usize = db::setting_get(&conn, "task.max_concurrency")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        identity_db: Mutex::new(identity_conn),
        llm_override: None,
        undo: Mutex::new(undo::UndoStore::new()),
        lsp: lsp::LspContext::new(Arc::new(lsp::ProcessLspSpawner)),
        task_limiter: subagent::TaskLimiter::new(task_max_concurrency),
    });

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

async fn send<W: tokio::io::AsyncWrite + Unpin>(w: &mut W, r: &Response) -> Result<()> {
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
        let rc = key.as_deref().and_then(|k| db::repo_context_get(&db, k).ok().flatten());
        drop(db); // release regin.db before locking identity.db
        let idb = state.identity_db.lock().expect("DB poisoned");
        let hostname = identity_db::hostname();
        let mems = identity_db::context_memories(&idb, 50, Some(&hostname)).unwrap_or_default();
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
async fn agentic_chat<W: tokio::io::AsyncWrite + Unpin>(
    state: &Arc<AppState>,
    _conversation_id: &str,
    user_messages: &[ChatMessage],
    cwd: Option<&str>,
    w: &mut W,
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
        msgs.push(MimirClient::msg_to_value(m));
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

                    let result = tools::execute_tool_full(call, cwd, persona.as_ref(), &state.undo, &state.db, &state.lsp, client.as_ref(), &state.task_limiter).await;

                    send(w, &Response::ToolResultEvent {
                        name: result.name.clone(),
                        success: result.success,
                        output: result.output.clone(),
                    }).await?;

                    // Add tool result to conversation
                    msgs.push(MimirClient::tool_result_message(&result.tool_call_id, &result.output));
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

async fn dispatch<W: tokio::io::AsyncWrite + Unpin>(
    req: Request,
    state: &Arc<AppState>,
    w: &mut W,
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
            let loaded = skills::load_skill(&config::system_skills_dir(), &config::user_skills_dir()?, &skill)?;
            // FEAT-047: "default" resolves the skill's declared cadence, overridden
            // by a to-be-state per-domain tune; an explicit interval wins outright.
            let interval = if interval == "default" {
                let skill_default = schedule::parse_skill_cadence(&loaded.prompt);
                let tune = desired::cadence_tune(
                    &config::system_desired_dir(), &config::user_desired_dir().unwrap_or_default(), &skill,
                );
                schedule::resolve_cadence(skill_default.as_deref(), None, tune.as_deref())
                    .ok_or_else(|| anyhow!("no cadence declared for '{skill}' (pass an explicit interval)"))?
            } else {
                interval
            };
            let next_run = compute_next_run(&interval, &skill)?;
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

            // Record user messages in identity.db transcript (FEAT-023).
            if let Some(user_msg) = messages.iter().rev().find(|m| m.role == "user") {
                let idb = state.identity_db.lock().expect("DB poisoned");
                identity_db::transcript_append(&idb, &conversation_id, "user", &user_msg.content)?;
                // Update session title from first user message.
                let _ = idb.execute(
                    "UPDATE sessions SET title = ?1 WHERE id = ?2 AND title = ''",
                    rusqlite::params![&title, &conversation_id],
                );
            }

            let full = agentic_chat(state, &conversation_id, &messages, cwd.as_deref(), w).await?;

            {
                let idb = state.identity_db.lock().expect("DB poisoned");
                identity_db::transcript_append(&idb, &conversation_id, "assistant", &full)?;
                // Generate a compact summary from the title + first user message.
                let summary = if title != "Untitled" {
                    format!("Chat: {title}")
                } else {
                    format!("Chat reply ({} chars)", full.len())
                };
                identity_db::session_close(
                    &idb,
                    &conversation_id,
                    "chat",
                    None,
                    Some(&summary),
                    full.len() as u64,
                )?;
            }
            send(w, &Response::StreamDone { conversation_id }).await?;
        }

        Request::ChatNew => {
            let id = uuid::Uuid::new_v4().to_string();
            let hostname = identity_db::hostname();
            {
                let idb = state.identity_db.lock().expect("DB poisoned");
                identity_db::session_open_with_id(&idb, &id, "chat", Some(&hostname), "")?;
            }
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
            let mems = { let db = state.identity_db.lock().expect("DB poisoned"); identity_db::memory_list(&db, category.as_deref())? };
            send(w, &Response::MemoryList { memories: mems }).await?;
        }

        Request::MemorySearch { query } => {
            let hostname = identity_db::hostname();

            // Best-effort hybrid search with embeddings (FEAT-026).
            let embeddings_enabled = {
                let db = state.db.lock().expect("DB poisoned");
                db::setting_get(&db, "memory.embeddings.enabled").unwrap_or_else(|_| "true".into())
            };

            let mems = if embeddings_enabled == "true" {
                let embedding_model = {
                    let db = state.db.lock().expect("DB poisoned");
                    db::setting_get(&db, "memory.embeddings.model").unwrap_or_else(|_| "auto".into())
                };
                match state.llm_client() {
                    Ok(client) => {
                        match client.embedding(&query, &embedding_model).await {
                            Ok(q_emb) => {
                                let db = state.identity_db.lock().expect("DB poisoned");
                                identity_db::hybrid_search_ranked(&db, &query, &q_emb, Some(&hostname), 50)?
                            }
                            Err(e) => {
                                warn!("embedding failed, falling back to FTS: {e}");
                                let db = state.identity_db.lock().expect("DB poisoned");
                                identity_db::memory_search_ranked(&db, &query, Some(&hostname), 50)?
                            }
                        }
                    }
                    Err(_) => {
                        let db = state.identity_db.lock().expect("DB poisoned");
                        identity_db::memory_search_ranked(&db, &query, Some(&hostname), 50)?
                    }
                }
            } else {
                let db = state.identity_db.lock().expect("DB poisoned");
                identity_db::memory_search_ranked(&db, &query, Some(&hostname), 50)?
            };

            send(w, &Response::MemoryList { memories: mems }).await?;
        }

        Request::MemorySave { category, content } => {
            let m = { let db = state.identity_db.lock().expect("DB poisoned"); identity_db::memory_save(&db, &category, &content)? };
            // Best-effort embedding (fire-and-forget, FEAT-026).
            let s = state.clone();
            let c = content.clone();
            let mid = m.id.clone();
            tokio::spawn(async move {
                if let Err(e) = state_embed_memory(&s, &mid, &c).await {
                    tracing::debug!("embedding on save: {e}");
                }
            });
            send(w, &Response::Ok { message: format!("Memory saved: {} [{}]", m.id, m.category) }).await?;
        }

        Request::MemoryUpdate { id, content } => {
            { let db = state.identity_db.lock().expect("DB poisoned"); identity_db::memory_update(&db, &id, &content)?; }
            // Best-effort re-embed (FEAT-026).
            let s = state.clone();
            let c = content.clone();
            let mid = id.clone();
            tokio::spawn(async move {
                if let Err(e) = state_embed_memory(&s, &mid, &c).await {
                    tracing::debug!("embedding on update: {e}");
                }
            });
            send(w, &Response::Ok { message: format!("Memory {id} updated") }).await?;
        }

        Request::MemoryDelete { id } => {
            { let db = state.identity_db.lock().expect("DB poisoned"); identity_db::memory_delete(&db, &id)?; }
            send(w, &Response::Ok { message: format!("Memory {id} deleted") }).await?;
        }

        // --- Portability (FEAT-027) ---
        Request::MemoryExport { path } => {
            { let db = state.identity_db.lock().expect("DB poisoned"); identity_db::memory_export(&db, &path)?; }
            send(w, &Response::MemoryExport { path }).await?;
        }

        Request::MemoryImport { path, merge } => {
            let count = { let db = state.identity_db.lock().expect("DB poisoned"); identity_db::memory_import(&db, &path, merge)? };
            send(w, &Response::Ok { message: format!("Imported {count} memories from {path}") }).await?;
        }

        Request::MemoryInfo => {
            let info = { let db = state.identity_db.lock().expect("DB poisoned"); identity_db::memory_info(&db)? };
            send(w, &Response::MemoryInfo {
                identity_id: info.identity_id,
                name: info.name,
                host: info.host,
                schema_version: info.schema_version,
                memory_count: info.memory_count,
                created_at: info.created_at,
            }).await?;
        }

        // --- Curator / reflection (FEAT-006 / FEAT-024) ---
        Request::MemoryReflect => {
            let stats = run_curation(state).await?;
            send(w, &Response::ReflectStats {
                episodes: stats.episodes as u32,
                reinforced: stats.added as u32,
                created: stats.added as u32,
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

        // --- CSI metrics (FEAT-050) ---
        Request::Metrics { since_days } => {
            let days = since_days.unwrap_or(30).max(1) as i64;
            let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
            let (summary, objective, intent_rag) = {
                let db = state.db.lock().expect("DB poisoned");
                let floor: f64 = db::setting_get(&db, "kpi.reliability_floor")?
                    .parse()
                    .unwrap_or(0.95);
                let summary = kpi::summary(&db, &since)?;
                let objective = kpi::objective(&summary, floor);
                let intent_rag = greeting::intent_rag_summary(&db)?;
                (summary, objective, intent_rag)
            };
            send(w, &Response::Metrics { summary: Box::new(summary), objective, intent_rag }).await?;
        }

        // --- Notice filters (FEAT-052) ---
        Request::FiltersList => {
            let rules = filters::load_filters(&config::system_filters_dir(), &config::user_filters_dir()?);
            send(w, &Response::Filters { rules }).await?;
        }
        Request::FiltersTest { domain, text } => {
            let rules = filters::load_filters(&config::system_filters_dir(), &config::user_filters_dir()?);
            let message = match filters::first_match(&rules, &domain, &text) {
                Some(r) => format!("FILTERED by rule `{}` (would be dropped before the LLM)", r.name),
                None => "NOT filtered (would reach the LLM review tier)".to_string(),
            };
            send(w, &Response::Ok { message }).await?;
        }

        // --- Effective mode (FEAT-041) ---
        Request::ModeQuery => {
            let configured = bus::BusClient::from_env().is_ok();
            let (last_ok, failures) = {
                let db = state.db.lock().expect("DB poisoned");
                let last_ok = db::setting_get(&db, "bus.last_ok").ok().filter(|s| !s.is_empty());
                let failures: u32 = db::setting_get(&db, "bus.failures").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
                (last_ok, failures)
            };
            let reach = mode::ReachabilityState {
                last_ok: last_ok.as_deref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.with_timezone(&chrono::Utc)),
                consecutive_failures: failures,
            };
            let m = mode::effective_mode(configured, &reach, chrono::Utc::now(), mode::ModePolicy::default());
            send(w, &Response::ModeInfo { mode: m.to_string(), configured, last_ok, failures }).await?;
        }

        // --- Adaptive posture (FEAT-040) ---
        Request::PostureQuery => {
            let since = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
            let (summary, policy) = {
                let db = state.db.lock().expect("DB poisoned");
                let g = |k: &str| db::setting_get(&db, k).unwrap_or_default();
                let policy = posture::PosturePolicy {
                    allow_auto: g("posture.allow_auto") == "true",
                    min_samples: g("posture.min_samples").parse().unwrap_or(10),
                    min_success_rate: g("posture.min_success_rate").parse().unwrap_or(0.9),
                    max_promotion_error_rate: g("posture.max_promotion_error_rate").parse().unwrap_or(0.1),
                };
                (kpi::summary(&db, &since)?, policy)
            };
            let p = posture::compute(&summary, policy);
            send(w, &Response::PostureInfo {
                posture: p.to_string(),
                allow_auto: policy.allow_auto,
                change_successes: summary.change_successes,
                change_failures: summary.change_failures,
                change_success_rate: summary.change_success_rate,
                promotion_error_rate: summary.promotion_error_rate,
            }).await?;
        }

        // --- Login greeting (FEAT-043) ---
        Request::GreetingQuery => {
            let configured = bus::BusClient::from_env().is_ok();
            let g = {
                let db = state.db.lock().expect("DB poisoned");
                let last_ok = db::setting_get(&db, "bus.last_ok").ok().filter(|s| !s.is_empty());
                let failures: u32 = db::setting_get(&db, "bus.failures").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
                let reach = mode::ReachabilityState {
                    last_ok: last_ok.as_deref()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|d| d.with_timezone(&chrono::Utc)),
                    consecutive_failures: failures,
                };
                let m = mode::effective_mode(configured, &reach, chrono::Utc::now(), mode::ModePolicy::default());
                greeting::build(&db, &m.to_string())?
            };
            send(w, &Response::GreetingResp { greeting: Box::new(g) }).await?;
        }

        // --- Active push (FEAT-044) ---
        Request::PushTest => {
            let (channel, target) = {
                let db = state.db.lock().expect("DB poisoned");
                (db::setting_get(&db, "push.channel")?, db::setting_get(&db, "push.target")?)
            };
            let ch = push::Channel::parse(&channel);
            match push::send(ch, &target, "regin test", "active-push test notification (FEAT-044)").await {
                Ok(()) => send(w, &Response::Ok { message: format!("Test notification sent via {channel}") }).await?,
                Err(e) => send(w, &Response::Error { message: format!("Push failed: {e}") }).await?,
            }
        }

        // --- Promoted deterministic checks (FEAT-051) ---
        Request::ChecksList => {
            let checks = { let db = state.db.lock().expect("DB poisoned"); promotion::active_checks(&db)? };
            send(w, &Response::DerivedChecks { checks }).await?;
        }

        // --- Self-audit (FEAT-055) ---
        Request::AuditRun => {
            let skill_domains: Vec<String> = opskill::load_all(
                &config::system_operator_skills_dir(),
                &config::user_operator_skills_dir().unwrap_or_default(),
            ).into_iter().map(|s| s.domain).collect();
            let desired_domains: Vec<String> = desired::load_all_desired(
                &config::system_desired_dir(),
                &config::user_desired_dir().unwrap_or_default(),
            ).into_iter().map(|d| d.domain).collect();
            let since = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
            let (report, opened) = {
                let db = state.db.lock().expect("DB poisoned");
                let floor: f64 = db::setting_get(&db, "kpi.reliability_floor")?.parse().unwrap_or(0.95);
                let summary = kpi::summary(&db, &since)?;
                let report = audit::run_audit(&summary, floor, &skill_domains, &desired_domains, false);
                let opened = audit::file_findings(&db, &report)?;
                (report, opened)
            };
            send(w, &Response::AuditResult { findings: report.findings, trimmed: report.trimmed, opened }).await?;
        }

        // --- Soul configurator + value catalog (FEAT-030) ---
        Request::SoulValuesList => {
            let cat = soul::catalog();
            send(w, &Response::SoulValues { version: cat.version.clone(), values: cat.values.clone() }).await?;
        }
        Request::SoulValuesShow { id } => match soul::find(&id) {
            Some(value) => send(w, &Response::SoulValueDetail { value: value.clone() }).await?,
            None => send(w, &Response::Error { message: format!("unknown value id {id:?} — see `regin soul values list`") }).await?,
        },
        Request::SoulCharterShow => {
            let core_ids = { let idb = state.identity_db.lock().expect("DB poisoned"); soul::charter_core_ids(&idb)? };
            let persona = regin_core::persona::Persona::from_env().unwrap_or(None);
            let persona_overlay = persona.map(|p| p.values).unwrap_or_default();
            let grounding = soul::grounding_union(&core_ids, &persona_overlay);
            send(w, &Response::SoulCharter { core_ids, persona_overlay, grounding }).await?;
        }
        Request::SoulCharterDerive => {
            let persona = regin_core::persona::Persona::from_env().unwrap_or(None);
            let role = persona.map(|p| p.role).unwrap_or_else(|| "operator".to_string());
            let proposed = soul::role_default_values(&role).into_iter().map(str::to_string).collect();
            send(w, &Response::SoulCharterProposal { role, proposed }).await?;
        }
        Request::SoulCharterConfirm { value_ids } => {
            let ids: Vec<&str> = value_ids.iter().map(String::as_str).collect();
            let result = {
                let idb = state.identity_db.lock().expect("DB poisoned");
                soul::charter_seed(&idb, &ids)
            };
            match result {
                Ok(created) => {
                    let added = created.into_iter().filter_map(|m| m.content.split_once(':').map(|(id, _)| id.to_string())).collect();
                    send(w, &Response::SoulCharterWritten { added }).await?;
                }
                Err(e) => send(w, &Response::Error { message: e.to_string() }).await?,
            }
        }
        Request::SoulCharterRemove { value_id } => {
            let removed = {
                let idb = state.identity_db.lock().expect("DB poisoned");
                soul::charter_remove(&idb, &value_id)?
            };
            if removed {
                send(w, &Response::Ok { message: format!("removed {value_id} from the core charter") }).await?;
            } else {
                send(w, &Response::Error { message: format!("{value_id} is not in the core charter") }).await?;
            }
        }

        // --- Principle derivation & ratification (FEAT-031) ---
        Request::SoulPrinciplesList { candidates_only } => {
            let principles = {
                let idb = state.identity_db.lock().expect("DB poisoned");
                if candidates_only { soul::principles_candidates(&idb)? } else { soul::principles_all(&idb)? }
            };
            send(w, &Response::SoulPrinciples { principles }).await?;
        }
        Request::SoulPrinciplesRatify { id } => {
            let result = {
                let idb = state.identity_db.lock().expect("DB poisoned");
                soul::principles_ratify(&idb, &id)
            };
            match result {
                Ok(principle) => send(w, &Response::SoulPrincipleRatified { principle }).await?,
                Err(e) => send(w, &Response::Error { message: e.to_string() }).await?,
            }
        }
        Request::SoulPrinciplesReject { id } => {
            let result = {
                let idb = state.identity_db.lock().expect("DB poisoned");
                soul::principles_reject(&idb, &id)
            };
            match result {
                Ok(principle) => send(w, &Response::SoulPrincipleRejected { principle }).await?,
                Err(e) => send(w, &Response::Error { message: e.to_string() }).await?,
            }
        }

        // --- Intent plane: objectives & goals (FEAT-069) ---
        Request::ObjectiveList => {
            let objectives = {
                let db = state.db.lock().expect("DB poisoned");
                objective::objective_list(&db)?
            };
            send(w, &Response::Objectives { objectives }).await?;
        }
        Request::ObjectiveShow { id } => {
            let found = {
                let db = state.db.lock().expect("DB poisoned");
                objective::objective_get(&db, &id)?
            };
            match found {
                Some(objective) => send(w, &Response::ObjectiveDetail { objective: Box::new(objective) }).await?,
                None => send(w, &Response::Error { message: format!("no objective {id}") }).await?,
            }
        }
        Request::GoalList { status } => {
            let goals = {
                let db = state.db.lock().expect("DB poisoned");
                goal::goal_list(&db, status.as_deref())?
            };
            send(w, &Response::Goals { goals }).await?;
        }
        Request::GoalShow { id } => {
            let found = {
                let db = state.db.lock().expect("DB poisoned");
                goal::goal_get(&db, &id)?
            };
            match found {
                Some(goal) => send(w, &Response::GoalDetail { goal: Box::new(goal) }).await?,
                None => send(w, &Response::Error { message: format!("no goal {id}") }).await?,
            }
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
async fn exec_skill_agentic<W: tokio::io::AsyncWrite + Unpin>(
    skill: &skills::Skill,
    state: &Arc<AppState>,
    cwd: Option<&str>,
    w: &mut W,
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

/// One scheduler tick: stamp the heartbeat, find schedules due at `now`, and
/// run each due skill — best-effort, one skill's failure never stops the
/// others or the loop (FEAT-073). Extracted from `schedule_checker`'s
/// `loop {}` body so due-vs-not-due, success/failure, and fail-safe
/// behaviour are unit-testable without a real 30s timer.
async fn run_due_schedules(state: &Arc<AppState>, now: &str) {
    let due = {
        let db = state.db.lock().expect("DB poisoned");
        // FEAT-048: stamp the scheduler heartbeat so a stalled loop is detectable.
        let _ = db::setting_set(&db, "regind.heartbeat", now);
        match db::get_due_schedules(&db, now) { Ok(s) => s, Err(e) => { error!("Schedule check: {e}"); return; } }
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
                let default_threshold = db::setting_get(&db, "monitor.recurrence_threshold")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(3);
                // FEAT-036: the domain's to-be-state may override the global default.
                let user_desired = config::user_desired_dir().unwrap_or_default();
                let threshold = desired::recurrence_threshold(
                    &config::system_desired_dir(), &user_desired, &sched.skill, default_threshold,
                );
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
        if let Ok(next) = compute_next_run(&sched.interval, &sched.skill) {
            let db = state.db.lock().expect("DB poisoned");
            let _ = db::update_schedule_after_run(&db, &sched.skill, &last_run, &next);
        }
    }
}

async fn schedule_checker(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        run_due_schedules(&state, &chrono::Utc::now().to_rfc3339()).await;
    }
}

/// Compute and store an embedding for a single memory (best-effort, FEAT-026).
/// Fails gracefully — errors are logged by callers.
async fn state_embed_memory(state: &Arc<AppState>, id: &str, content: &str) -> Result<()> {
    let enabled = {
        let db = state.db.lock().expect("DB poisoned");
        db::setting_get(&db, "memory.embeddings.enabled").unwrap_or_else(|_| "true".into())
    };
    if enabled != "true" {
        return Ok(());
    }
    let client = state.llm_client()?;
    let model = {
        let db = state.db.lock().expect("DB poisoned");
        db::setting_get(&db, "memory.embeddings.model").unwrap_or_else(|_| "auto".into())
    };
    let embedding = client.embedding(content, &model).await?;
    let idb = state.identity_db.lock().expect("DB poisoned");
    identity_db::store_memory_embedding(&idb, id, &embedding)?;
    Ok(())
}

/// Backfill embeddings for memories that don't have one yet (FEAT-026).
/// Processes up to `batch_size` memories per call, returns count embedded.
async fn backfill_embeddings(state: &Arc<AppState>, batch_size: usize) -> Result<usize> {
    let pending = {
        let idb = state.identity_db.lock().expect("DB poisoned");
        identity_db::memories_pending_embedding(&idb, batch_size)?
    };
    if pending.is_empty() {
        return Ok(0);
    }
    let mut count = 0usize;
    for (id, content) in &pending {
        if let Err(e) = state_embed_memory(state, id, content).await {
            info!("embedding backfill skipped {id}: {e}");
        } else {
            count += 1;
        }
    }
    Ok(count)
}

/// Run one curation pass (FEAT-024). The DB lock is released around the
/// network call so the daemon stays responsive.
async fn run_curation(state: &Arc<AppState>) -> Result<regin_core::types::CuratorStats> {
    let client = state.llm_client()?;
    let episode_window = 100usize;
    let transcript_window = 20usize;
    let (episodes, existing, sessions, decay_before, prune_before, principle_decay_before, recurrence_threshold) = {
        let db = state.db.lock().expect("DB poisoned");
        let decay_days = db::setting_get(&db, "memory.decay_days")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(30);
        let decay_before = (chrono::Utc::now() - chrono::Duration::days(decay_days)).to_rfc3339();
        // Active principles decay slower than ordinary reflection memories
        // (FEAT-031 acceptance criterion 4) — a longer, more lenient window.
        let principle_decay_before = (chrono::Utc::now() - chrono::Duration::days(decay_days * 3)).to_rfc3339();
        let prune_days = db::setting_get(&db, "memory.prune_days")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(90);
        let prune_before = (chrono::Utc::now() - chrono::Duration::days(prune_days)).to_rfc3339();
        let recurrence_threshold = db::setting_get(&db, "decision.principles.recurrence_threshold")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(3);
        let idb = state.identity_db.lock().expect("DB poisoned");
        let (e, ex, sess) = reflect::gather_curation_inputs(&idb, episode_window, transcript_window)?;
        drop(idb);
        drop(db);
        (e, ex, sess, decay_before, prune_before, principle_decay_before, recurrence_threshold)
    };

    if episodes.is_empty() && sessions.is_empty() {
        // Nothing to curate — still run maintenance + embedding backfill.
        let stats = {
            let idb = state.identity_db.lock().expect("DB poisoned");
            reflect::post_curation_maintenance(&idb, &decay_before, 5, &prune_before, &principle_decay_before, recurrence_threshold)?
        };
        if let Ok(n) = backfill_embeddings(state, 10).await {
            if n > 0 {
                info!("backfilled {n} embeddings");
            }
        }
        return Ok(stats);
    }

    let prompt = reflect::curation_prompt(&episodes, &existing, &sessions);
    let text = client.chat_completion(&[ChatMessage::user(prompt)]).await?;
    let proposals = reflect::parse_curator_proposals(&text).unwrap_or_default();

    let topics: Vec<String> = proposals.iter()
        .filter_map(|p| p.topic.as_ref().filter(|t| !t.is_empty()).cloned())
        .collect();

    let stats = {
        let idb = state.identity_db.lock().expect("DB poisoned");
        let mut s = reflect::apply_curation(&idb, &proposals)?;
        let mark = reflect::mark_consolidated(&idb, &episodes, &sessions, &topics)?;
        s.episodes = mark.episodes;
        s.sessions = mark.sessions;
        s.topics = mark.topics;
        let maint = reflect::post_curation_maintenance(&idb, &decay_before, 5, &prune_before, &principle_decay_before, recurrence_threshold)?;
        s.promoted = maint.promoted;
        s.decayed = maint.decayed;
        s.pruned = maint.pruned;
        s.principles_proposed = maint.principles_proposed;
        s
    };

    // Embedding backfill for new/modified memories (FEAT-026).
    if let Ok(n) = backfill_embeddings(state, 10).await {
        if n > 0 {
            info!("backfilled {n} embeddings after curation");
        }
    }

    Ok(stats)
}

/// One reflection-checker tick: run curation and log its result (or a
/// non-fatal warning on failure) — returning the outcome so tests can
/// observe it directly rather than only through log lines. Extracted from
/// `reflection_checker`'s `loop {}` body (FEAT-073) so success/failure is
/// unit-testable without the interval sleep.
async fn reflection_tick(state: &Arc<AppState>) -> Result<regin_core::types::CuratorStats> {
    let result = run_curation(state).await;
    match &result {
        Ok(s) if s.episodes > 0 || s.sessions > 0 || s.decayed > 0 || s.promoted > 0 => info!(
            episodes = s.episodes,
            sessions = s.sessions,
            added = s.added,
            updated = s.updated,
            deleted = s.deleted,
            promoted = s.promoted,
            decayed = s.decayed,
            pruned = s.pruned,
            topics = s.topics,
            "curation pass"
        ),
        Ok(_) => {}
        Err(e) => warn!("curation: {e}"),
    }
    result
}

/// Periodically curate episodes and transcripts (FEAT-024).
/// Fails safe — errors are logged and never stop the loop.
async fn reflection_checker(state: Arc<AppState>) {
    loop {
        let interval_str = {
            let db = state.db.lock().expect("DB poisoned");
            db::setting_get(&db, "memory.reflect_interval").unwrap_or_else(|_| "daily".into())
        };
        let dur = schedule::parse_interval(&interval_str)
            .ok()
            .and_then(|d| d.to_std().ok())
            .unwrap_or_else(|| std::time::Duration::from_secs(86_400));
        tokio::time::sleep(dur).await;

        let _ = reflection_tick(&state).await;
    }
}

/// Next run for a skill, with deterministic per-skill jitter to smooth load
/// (FEAT-047). Up to 10% of the interval is added, staggered by skill name.
fn compute_next_run(interval: &str, skill: &str) -> Result<String> {
    Ok(schedule::next_run_with_jitter(interval, skill, 0.1, chrono::Utc::now())?.to_rfc3339())
}

async fn shutdown_signal() {
    let ctrl_c = signal::ctrl_c();
    #[cfg(unix)]
    let mut term = signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM");
    tokio::select! { _ = ctrl_c => {} _ = term.recv() => {} }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use regin_core::db;
    use tokio::io::AsyncReadExt;

    fn state() -> Arc<AppState> {
        state_with_llm(None)
    }

    /// FEAT-071: construct `AppState` with an injected `LlmClient` (typically
    /// a `FakeLlm`) so chat/task-exec dispatch arms can be driven end-to-end
    /// without a network call.
    fn state_with_llm(llm: Option<Arc<dyn LlmClient>>) -> Arc<AppState> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_schema(&conn).unwrap();
        let identity_conn = rusqlite::Connection::open_in_memory().unwrap();
        identity_db::init_identity_schema(&identity_conn).unwrap();
        Arc::new(AppState {
            db: Mutex::new(conn),
            identity_db: Mutex::new(identity_conn),
            llm_override: llm,
            undo: Mutex::new(undo::UndoStore::new()),
            lsp: lsp::LspContext::new(Arc::new(lsp::ProcessLspSpawner)),
            task_limiter: subagent::TaskLimiter::new(3),
        })
    }

    /// Drive the real dispatch over an in-memory duplex and collect responses.
    async fn run(req: Request, st: &Arc<AppState>) -> Vec<Response> {
        let (mut client, mut server) = tokio::io::duplex(256 * 1024);
        dispatch(req, st, &mut server).await.unwrap();
        drop(server); // EOF for the reader
        let mut out = String::new();
        client.read_to_string(&mut out).await.unwrap();
        out.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<Response>(l).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn ping_pongs() {
        let st = state();
        assert!(matches!(run(Request::Ping, &st).await.as_slice(), [Response::Pong]));
    }

    #[tokio::test]
    async fn itil_incident_and_change_flow() {
        let st = state();
        // open an incident, then list it
        let r = run(Request::IncidentOpen { title: "disk full".into(), description: "x".into(), severity: "high".into() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::Ok { .. }]));
        let listed = run(Request::IncidentList { status: None }, &st).await;
        match listed.as_slice() {
            [Response::Incidents { incidents }] => assert_eq!(incidents.len(), 1),
            other => panic!("expected one incident, got {other:?}"),
        }
        // record a change, then list it
        run(Request::ChangeRecord { title: "fix".into(), description: "".into(), incident_id: None, problem_id: None, before: None, after: None }, &st).await;
        let changes = run(Request::ChangeList, &st).await;
        match changes.as_slice() {
            [Response::Changes { changes }] => assert_eq!(changes.len(), 1),
            other => panic!("expected one change, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_only_queries_respond() {
        let st = state();
        assert!(matches!(run(Request::Metrics { since_days: Some(7) }, &st).await.as_slice(), [Response::Metrics { .. }]));
        assert!(matches!(run(Request::ModeQuery, &st).await.as_slice(), [Response::ModeInfo { .. }]));
        assert!(matches!(run(Request::FiltersList, &st).await.as_slice(), [Response::Filters { .. }]));
        assert!(matches!(run(Request::DesiredList, &st).await.as_slice(), [Response::DesiredListResp { .. }]));
        assert!(matches!(run(Request::ProblemList { status: None }, &st).await.as_slice(), [Response::Problems { .. }]));
        assert!(matches!(run(Request::ObjectiveList, &st).await.as_slice(), [Response::Objectives { .. }]));
        assert!(matches!(run(Request::GoalList { status: None }, &st).await.as_slice(), [Response::Goals { .. }]));
    }

    #[tokio::test]
    async fn unknown_incident_show_errors() {
        let st = state();
        let r = run(Request::IncidentShow { id: "nope".into() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::Error { .. }]));
    }

    #[tokio::test]
    async fn objective_and_goal_show_round_trip_and_error_on_unknown_id() {
        let st = state();
        let obj = {
            let db = st.db.lock().unwrap();
            objective::objective_create(
                &db, "t", "d", "m", "sum", 30, "le", &regin_core::desired::AssertValue::Num(1.0), 1, "human",
            ).unwrap()
        };
        let r = run(Request::ObjectiveShow { id: obj.id.clone() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::ObjectiveDetail { .. }]));
        let r = run(Request::ObjectiveShow { id: "no-such-id".into() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::Error { .. }]));

        let deadline = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        let g = {
            let db = st.db.lock().unwrap();
            goal::goal_create(&db, "d", "t", &deadline, vec![], 1, "human").unwrap()
        };
        let r = run(Request::GoalShow { id: g.id.clone() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::GoalDetail { .. }]));
        let r = run(Request::GoalShow { id: "no-such-id".into() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::Error { .. }]));
    }

    #[tokio::test]
    async fn send_serializes_one_line_per_response() {
        let mut buf: Vec<u8> = Vec::new();
        // Vec<u8> implements tokio AsyncWrite
        send(&mut buf, &Response::Pong).await.unwrap();
        send(&mut buf, &Response::Ok { message: "hi".into() }).await.unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s.lines().count(), 2);
    }

    // -----------------------------------------------------------------
    // FEAT-071: chat dispatch end-to-end with an injected FakeLlm — no
    // network. `Request::TaskExec` shares the same `agentic_chat` LLM loop
    // (via `exec_skill_agentic`), so this exercises the LLM-dependent code
    // path both commands rely on.
    // -----------------------------------------------------------------

    use regin_core::llm::FakeLlm;

    #[tokio::test]
    async fn chat_send_uses_the_injected_llm_client() {
        let fake = Arc::new(FakeLlm::new());
        fake.push_turn(LlmTurn::Text("hello from fake".into()));
        let st = state_with_llm(Some(fake as Arc<dyn LlmClient>));

        let conv_id = match run(Request::ChatNew, &st).await.as_slice() {
            [Response::ChatNew { conversation_id }] => conversation_id.clone(),
            other => panic!("expected ChatNew, got {other:?}"),
        };

        let resp = run(
            Request::ChatSend { conversation_id: conv_id, messages: vec![ChatMessage::user("hi")], cwd: None },
            &st,
        ).await;

        assert!(
            resp.iter().any(|r| matches!(r, Response::StreamChunk { token } if token == "hello from fake")),
            "expected a StreamChunk carrying the fake's reply, got {resp:?}"
        );
        assert!(matches!(resp.last(), Some(Response::StreamDone { .. })));
    }

    #[tokio::test]
    async fn chat_send_without_a_queued_llm_reply_errors_without_touching_the_network() {
        // dispatch() propagates agentic_chat's error via `?` rather than
        // sending a Response::Error on the wire (that's the connection
        // handler's job upstream), so assert on dispatch()'s own Result
        // instead of going through the `run()` helper (which unwraps it).
        let fake = Arc::new(FakeLlm::new()); // no queued turn
        let st = state_with_llm(Some(fake as Arc<dyn LlmClient>));

        let conv_id = match run(Request::ChatNew, &st).await.as_slice() {
            [Response::ChatNew { conversation_id }] => conversation_id.clone(),
            other => panic!("expected ChatNew, got {other:?}"),
        };
        let mut sink: Vec<u8> = Vec::new();
        let result = dispatch(
            Request::ChatSend { conversation_id: conv_id, messages: vec![ChatMessage::user("hi")], cwd: None },
            &st,
            &mut sink,
        ).await;
        assert!(result.is_err(), "expected dispatch to surface the FakeLlm's empty-queue error");
    }

    #[tokio::test]
    async fn llm_client_without_override_requires_a_configured_fingerprint() {
        // Production path (no override): still reads config fresh, still
        // refuses to build a client with no fingerprint set.
        let st = state();
        assert!(st.llm_client().is_err());
    }

    // -----------------------------------------------------------------
    // FEAT-073: scheduler + reflection tick functions (acceptance criterion 1)
    // -----------------------------------------------------------------

    /// Creates a skill under the REAL user skills dir for a test's duration
    /// and removes it on drop (even on panic). There is no DI seam for
    /// `config::user_skills_dir()` — `run_due_schedules` calls it directly,
    /// same as production — so this is the only way to exercise the actual
    /// success path of a scheduled skill run. The name is unique per call to
    /// avoid any collision with a real skill or another test.
    struct TempSkillGuard {
        name: String,
    }
    impl TempSkillGuard {
        fn new(name: &str, prompt: &str) -> Self {
            let dir = config::user_skills_dir().unwrap().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("skill.md"), prompt).unwrap();
            Self { name: name.to_string() }
        }
    }
    impl Drop for TempSkillGuard {
        fn drop(&mut self) {
            if let Ok(dir) = config::user_skills_dir() {
                let _ = std::fs::remove_dir_all(dir.join(&self.name));
            }
        }
    }

    #[tokio::test]
    async fn run_due_schedules_stamps_the_heartbeat_even_with_nothing_due() {
        let st = state();
        let now = chrono::Utc::now().to_rfc3339();
        run_due_schedules(&st, &now).await;
        let db = st.db.lock().unwrap();
        assert_eq!(db::setting_get(&db, "regind.heartbeat").unwrap(), now);
    }

    #[tokio::test]
    async fn run_due_schedules_skips_a_schedule_not_yet_due() {
        let st = state();
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        { let db = st.db.lock().unwrap(); db::save_schedule(&db, "feat073-not-due", "daily", &future).unwrap(); }

        run_due_schedules(&st, &chrono::Utc::now().to_rfc3339()).await;

        let db = st.db.lock().unwrap();
        assert!(db::get_task_runs(&db, "feat073-not-due", 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn run_due_schedules_is_fail_safe_when_the_skill_cannot_be_loaded() {
        // No skill file exists for this name anywhere — load_skill errors;
        // the tick must log and continue, not panic or stop other schedules.
        let st = state();
        let name = format!("feat073-missing-{}", uuid::Uuid::new_v4());
        let past = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        { let db = st.db.lock().unwrap(); db::save_schedule(&db, &name, "daily", &past).unwrap(); }

        run_due_schedules(&st, &chrono::Utc::now().to_rfc3339()).await; // must not panic

        let db = st.db.lock().unwrap();
        assert!(db::get_task_runs(&db, &name, 10).unwrap().is_empty(), "no run recorded for an unloadable skill");
    }

    #[tokio::test]
    async fn run_due_schedules_runs_a_due_skill_and_advances_next_run() {
        let name = format!("feat073-success-{}", uuid::Uuid::new_v4());
        let _guard = TempSkillGuard::new(&name, "a temp scheduler-tick test skill\n\nsay hi.");
        let fake = Arc::new(FakeLlm::new());
        fake.push_completion("scheduled reply");
        let st = state_with_llm(Some(fake as Arc<dyn LlmClient>));
        let past = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        { let db = st.db.lock().unwrap(); db::save_schedule(&db, &name, "daily", &past).unwrap(); }

        run_due_schedules(&st, &chrono::Utc::now().to_rfc3339()).await;

        let db = st.db.lock().unwrap();
        let runs = db::get_task_runs(&db, &name, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert_eq!(runs[0].output, "scheduled reply");
        let next_run = db::list_schedules(&db).unwrap().into_iter().find(|s| s.skill == name).unwrap().next_run;
        assert!(next_run > past, "next_run advanced past the due time");
    }

    #[tokio::test]
    async fn run_due_schedules_records_a_failure_status_when_the_llm_errors() {
        let name = format!("feat073-failure-{}", uuid::Uuid::new_v4());
        let _guard = TempSkillGuard::new(&name, "a temp scheduler-tick test skill\n\nsay hi.");
        let fake = Arc::new(FakeLlm::new()); // no queued completion -> chat_completion errors
        let st = state_with_llm(Some(fake as Arc<dyn LlmClient>));
        let past = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        { let db = st.db.lock().unwrap(); db::save_schedule(&db, &name, "daily", &past).unwrap(); }

        run_due_schedules(&st, &chrono::Utc::now().to_rfc3339()).await;

        let db = st.db.lock().unwrap();
        let runs = db::get_task_runs(&db, &name, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "error");
    }

    #[tokio::test]
    async fn reflection_tick_returns_stats_on_an_empty_db() {
        let fake = Arc::new(FakeLlm::new());
        let st = state_with_llm(Some(fake as Arc<dyn LlmClient>));
        let stats = reflection_tick(&st).await.unwrap();
        assert_eq!(stats.episodes, 0);
    }

    #[tokio::test]
    async fn reflection_tick_surfaces_the_error_when_no_llm_is_configured() {
        let st = state(); // no override, no fingerprint -> llm_client() errors
        assert!(reflection_tick(&st).await.is_err());
    }

    // -----------------------------------------------------------------
    // FEAT-073: full dispatch-arm coverage (acceptance criterion 2)
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn config_list_get_set_roundtrip() {
        let st = state();
        let set = run(Request::ConfigSet { key: "kpi.reliability_floor".into(), value: "0.99".into() }, &st).await;
        assert!(matches!(set.as_slice(), [Response::Ok { .. }]));
        let got = run(Request::ConfigGet { key: "kpi.reliability_floor".into() }, &st).await;
        assert!(matches!(got.as_slice(), [Response::ConfigValue { value, .. }] if value == "0.99"));
        let listed = run(Request::ConfigList, &st).await;
        assert!(matches!(
            listed.as_slice(),
            [Response::ConfigEntries { entries }] if entries.iter().any(|(k, _)| k == "kpi.reliability_floor")
        ));
    }

    #[tokio::test]
    async fn chat_history_lists_conversations() {
        let st = state();
        assert!(matches!(run(Request::ChatHistory, &st).await.as_slice(), [Response::ChatHistory { .. }]));
    }

    #[tokio::test]
    async fn memory_save_list_search_update_delete_roundtrip() {
        let st = state();
        let saved = run(Request::MemorySave { category: "fact".into(), content: "the sky is blue".into() }, &st).await;
        assert!(matches!(saved.as_slice(), [Response::Ok { .. }]));

        let listed = run(Request::MemoryList { category: Some("fact".into()) }, &st).await;
        let id = match listed.as_slice() {
            [Response::MemoryList { memories }] => { assert_eq!(memories.len(), 1); memories[0].id.clone() }
            other => panic!("expected one memory, got {other:?}"),
        };

        // embeddings enabled by default but no fingerprint configured -> falls
        // back to FTS-only search, no network.
        let searched = run(Request::MemorySearch { query: "sky".into() }, &st).await;
        assert!(matches!(searched.as_slice(), [Response::MemoryList { memories }] if !memories.is_empty()));

        assert!(matches!(
            run(Request::MemoryUpdate { id: id.clone(), content: "the sky is grey".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(run(Request::MemoryDelete { id }, &st).await.as_slice(), [Response::Ok { .. }]));

        let after = run(Request::MemoryList { category: Some("fact".into()) }, &st).await;
        assert!(matches!(after.as_slice(), [Response::MemoryList { memories }] if memories.is_empty()));
    }

    #[tokio::test]
    async fn memory_info_reports_identity_metadata() {
        let st = state();
        assert!(matches!(run(Request::MemoryInfo, &st).await.as_slice(), [Response::MemoryInfo { .. }]));
    }

    #[tokio::test]
    async fn memory_export_then_import_merge_round_trips() {
        let st = state();
        let path = std::env::temp_dir().join(format!("regind-test-export-{}.db", uuid::Uuid::new_v4()));
        let path_str = path.to_string_lossy().to_string();

        let exported = run(Request::MemoryExport { path: path_str.clone() }, &st).await;
        assert!(matches!(exported.as_slice(), [Response::MemoryExport { .. }]));

        let imported = run(Request::MemoryImport { path: path_str, merge: true }, &st).await;
        assert!(matches!(imported.as_slice(), [Response::Ok { .. }]));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn memory_import_without_merge_refuses_a_different_identity() {
        // MemoryImport propagates identity_db::memory_import's error via `?`
        // (same pattern as ChatSend/SkillShow), so assert on dispatch()'s
        // own Result rather than going through the `run()` helper (which
        // unwraps).
        let st = state();
        let other = rusqlite::Connection::open_in_memory().unwrap();
        identity_db::init_identity_schema(&other).unwrap(); // fresh, different identity_id
        let path = std::env::temp_dir().join(format!("regind-test-other-identity-{}.db", uuid::Uuid::new_v4()));
        other.execute_batch(&format!("VACUUM INTO '{}'", path.to_string_lossy())).unwrap();

        let mut sink: Vec<u8> = Vec::new();
        let result = dispatch(
            Request::MemoryImport { path: path.to_string_lossy().to_string(), merge: false },
            &st,
            &mut sink,
        ).await;
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn memory_reflect_returns_stats_when_an_llm_is_configured() {
        let fake = Arc::new(FakeLlm::new());
        let st = state_with_llm(Some(fake as Arc<dyn LlmClient>));
        assert!(matches!(run(Request::MemoryReflect, &st).await.as_slice(), [Response::ReflectStats { .. }]));
    }

    #[tokio::test]
    async fn memory_reflect_errors_without_a_configured_llm() {
        let st = state();
        let mut sink: Vec<u8> = Vec::new();
        assert!(dispatch(Request::MemoryReflect, &st, &mut sink).await.is_err());
    }

    #[tokio::test]
    async fn itil_incident_lifecycle_update_resolve_block_close_and_show() {
        let st = state();
        run(Request::IncidentOpen { title: "t".into(), description: "d".into(), severity: "high".into() }, &st).await;
        let id = match run(Request::IncidentList { status: None }, &st).await.as_slice() {
            [Response::Incidents { incidents }] => incidents[0].id.clone(),
            other => panic!("expected one incident, got {other:?}"),
        };

        assert!(matches!(
            run(Request::IncidentShow { id: id.clone() }, &st).await.as_slice(),
            [Response::Incidents { incidents }] if incidents.len() == 1
        ));
        assert!(matches!(
            run(Request::IncidentUpdate { id: id.clone(), status: "investigating".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(
            run(Request::IncidentBlock { id: id.clone(), workaround: "manual restart".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(
            run(Request::IncidentResolve { id: id.clone(), resolution: "patched".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(run(Request::IncidentClose { id }, &st).await.as_slice(), [Response::Ok { .. }]));
    }

    #[tokio::test]
    async fn itil_change_lifecycle_request_approve_show_apply_close() {
        let st = state();
        run(Request::ChangeRecord {
            title: "c".into(), description: "d".into(), incident_id: None, problem_id: None, before: None, after: None,
        }, &st).await;
        let id = match run(Request::ChangeList, &st).await.as_slice() {
            [Response::Changes { changes }] => changes[0].id.clone(),
            other => panic!("expected one change, got {other:?}"),
        };

        assert!(matches!(
            run(Request::ChangeShow { id: id.clone() }, &st).await.as_slice(),
            [Response::Changes { changes }] if changes.len() == 1
        ));
        assert!(matches!(
            run(Request::ChangeRequestApproval { id: id.clone() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(
            run(Request::ChangeApprove { id: id.clone(), approved_by: "rene".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(run(Request::ChangeApply { id: id.clone() }, &st).await.as_slice(), [Response::Ok { .. }]));
        assert!(matches!(run(Request::ChangeClose { id }, &st).await.as_slice(), [Response::Ok { .. }]));
    }

    #[tokio::test]
    async fn itil_change_show_errors_on_unknown_id() {
        let st = state();
        assert!(matches!(
            run(Request::ChangeShow { id: "nope".into() }, &st).await.as_slice(),
            [Response::Error { .. }]
        ));
    }

    #[tokio::test]
    async fn itil_problem_lifecycle_link_known_error_hypotheses_close() {
        let st = state();
        run(Request::IncidentOpen { title: "t".into(), description: "d".into(), severity: "high".into() }, &st).await;
        let incident_id = match run(Request::IncidentList { status: None }, &st).await.as_slice() {
            [Response::Incidents { incidents }] => incidents[0].id.clone(),
            other => panic!("{other:?}"),
        };
        run(Request::ProblemOpen { title: "p".into(), description: "d".into() }, &st).await;
        let problem_id = match run(Request::ProblemList { status: None }, &st).await.as_slice() {
            [Response::Problems { problems }] => problems[0].id.clone(),
            other => panic!("{other:?}"),
        };

        assert!(matches!(
            run(Request::ProblemShow { id: problem_id.clone() }, &st).await.as_slice(),
            [Response::Problems { problems }] if problems.len() == 1
        ));
        assert!(matches!(
            run(Request::ProblemLink { problem_id: problem_id.clone(), incident_id }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(
            run(Request::ProblemKnownError { id: problem_id.clone(), root_cause: "disk".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));

        run(Request::ProblemHypothesisAdd { problem_id: problem_id.clone(), text: "maybe X".into() }, &st).await;
        let hyp_id = match run(Request::ProblemHypothesisList { problem_id: problem_id.clone() }, &st).await.as_slice() {
            [Response::Hypotheses { hypotheses }] => hypotheses[0].id.clone(),
            other => panic!("{other:?}"),
        };
        assert!(matches!(
            run(Request::ProblemHypothesisStatus { id: hyp_id, status: "confirmed".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(run(Request::ProblemClose { id: problem_id }, &st).await.as_slice(), [Response::Ok { .. }]));
    }

    #[tokio::test]
    async fn itil_problem_show_errors_on_unknown_id() {
        let st = state();
        assert!(matches!(
            run(Request::ProblemShow { id: "nope".into() }, &st).await.as_slice(),
            [Response::Error { .. }]
        ));
    }

    #[tokio::test]
    async fn desired_show_errors_on_unknown_domain() {
        let st = state();
        let r = run(Request::DesiredShow { domain: "definitely-not-a-real-domain-xyz".into() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::Error { .. }]));
    }

    #[tokio::test]
    async fn desired_check_reports_a_summary_message() {
        let st = state();
        assert!(matches!(run(Request::DesiredCheck, &st).await.as_slice(), [Response::Ok { .. }]));
    }

    #[tokio::test]
    async fn filters_test_reports_not_filtered_when_no_rules_match() {
        let st = state();
        let r = run(Request::FiltersTest { domain: "disk".into(), text: "some log line".into() }, &st).await;
        assert!(matches!(r.as_slice(), [Response::Ok { .. }]));
    }

    #[tokio::test]
    async fn posture_query_reports_a_posture() {
        let st = state();
        assert!(matches!(run(Request::PostureQuery, &st).await.as_slice(), [Response::PostureInfo { .. }]));
    }

    #[tokio::test]
    async fn greeting_query_builds_a_greeting() {
        let st = state();
        assert!(matches!(run(Request::GreetingQuery, &st).await.as_slice(), [Response::GreetingResp { .. }]));
    }

    #[tokio::test]
    async fn push_test_errors_without_a_configured_target() {
        let st = state();
        assert!(matches!(run(Request::PushTest, &st).await.as_slice(), [Response::Error { .. }]));
    }

    #[tokio::test]
    async fn checks_list_is_empty_on_a_fresh_db() {
        let st = state();
        assert!(matches!(
            run(Request::ChecksList, &st).await.as_slice(),
            [Response::DerivedChecks { checks }] if checks.is_empty()
        ));
    }

    #[tokio::test]
    async fn audit_run_produces_a_result_on_a_fresh_db() {
        let st = state();
        assert!(matches!(run(Request::AuditRun, &st).await.as_slice(), [Response::AuditResult { .. }]));
    }

    #[tokio::test]
    async fn soul_values_list_and_show_happy_and_error() {
        let st = state();
        assert!(matches!(run(Request::SoulValuesList, &st).await.as_slice(), [Response::SoulValues { .. }]));
        assert!(matches!(
            run(Request::SoulValuesShow { id: "integrity".into() }, &st).await.as_slice(),
            [Response::SoulValueDetail { .. }]
        ));
        assert!(matches!(
            run(Request::SoulValuesShow { id: "made-up".into() }, &st).await.as_slice(),
            [Response::Error { .. }]
        ));
    }

    #[tokio::test]
    async fn soul_charter_show_derive_confirm_remove() {
        let st = state();
        assert!(matches!(run(Request::SoulCharterShow, &st).await.as_slice(), [Response::SoulCharter { .. }]));
        assert!(matches!(run(Request::SoulCharterDerive, &st).await.as_slice(), [Response::SoulCharterProposal { .. }]));

        let written = run(Request::SoulCharterConfirm { value_ids: vec!["integrity".into()] }, &st).await;
        assert!(matches!(written.as_slice(), [Response::SoulCharterWritten { added }] if added == &vec!["integrity".to_string()]));

        assert!(matches!(
            run(Request::SoulCharterConfirm { value_ids: vec!["made-up".into()] }, &st).await.as_slice(),
            [Response::Error { .. }]
        ));
        assert!(matches!(
            run(Request::SoulCharterRemove { value_id: "integrity".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        assert!(matches!(
            run(Request::SoulCharterRemove { value_id: "integrity".into() }, &st).await.as_slice(),
            [Response::Error { .. }]
        ));
    }

    #[tokio::test]
    async fn soul_principles_list_and_ratify_a_real_candidate() {
        let st = state();
        let candidate_id = {
            let idb = st.identity_db.lock().unwrap();
            identity_db::principle_insert_candidate(&idb, "a reflection-proposed principle", &["ep1".into()]).unwrap().id
        };

        let candidates = run(Request::SoulPrinciplesList { candidates_only: true }, &st).await;
        assert!(matches!(candidates.as_slice(), [Response::SoulPrinciples { principles }] if principles.len() == 1));

        let ratified = run(Request::SoulPrinciplesRatify { id: candidate_id.clone() }, &st).await;
        assert!(matches!(ratified.as_slice(), [Response::SoulPrincipleRatified { principle }] if principle.status == "active"));

        // no longer a candidate, so ratifying again errors
        assert!(matches!(
            run(Request::SoulPrinciplesRatify { id: candidate_id }, &st).await.as_slice(),
            [Response::Error { .. }]
        ));
    }

    #[tokio::test]
    async fn soul_principles_reject_retires_and_is_not_repeatable() {
        let st = state();
        run(Request::SoulCharterConfirm { value_ids: vec!["integrity".into()] }, &st).await;
        let id = match run(Request::SoulPrinciplesList { candidates_only: false }, &st).await.as_slice() {
            [Response::SoulPrinciples { principles }] => principles[0].id.clone(),
            other => panic!("{other:?}"),
        };

        let rejected = run(Request::SoulPrinciplesReject { id: id.clone() }, &st).await;
        assert!(matches!(rejected.as_slice(), [Response::SoulPrincipleRejected { principle }] if principle.status == "retired"));
        assert!(matches!(
            run(Request::SoulPrinciplesReject { id }, &st).await.as_slice(),
            [Response::Error { .. }]
        ));
    }

    #[tokio::test]
    async fn skill_list_responds_regardless_of_ambient_skill_state() {
        let st = state();
        assert!(matches!(run(Request::SkillList { cwd: None }, &st).await.as_slice(), [Response::SkillList { .. }]));
    }

    #[tokio::test]
    async fn skill_show_and_task_exec_error_on_an_unknown_skill() {
        // dispatch() propagates the load error via `?` (same pattern as
        // ChatSend), so assert on dispatch()'s Result directly.
        let st = state();
        let name = format!("definitely-not-a-real-skill-{}", uuid::Uuid::new_v4());
        let mut sink: Vec<u8> = Vec::new();
        assert!(dispatch(Request::SkillShow { name: name.clone(), cwd: None }, &st, &mut sink).await.is_err());
        assert!(dispatch(Request::TaskExec { skill: name, cwd: None }, &st, &mut sink).await.is_err());
    }

    #[tokio::test]
    async fn task_schedule_errors_on_an_unknown_skill() {
        let st = state();
        let name = format!("definitely-not-a-real-skill-{}", uuid::Uuid::new_v4());
        let mut sink: Vec<u8> = Vec::new();
        assert!(dispatch(Request::TaskSchedule { skill: name, interval: "daily".into() }, &st, &mut sink).await.is_err());
    }

    #[tokio::test]
    async fn task_unschedule_and_schedules_list() {
        let st = state();
        assert!(matches!(run(Request::TaskUnschedule { skill: "nope".into() }, &st).await.as_slice(), [Response::Ok { .. }]));
        assert!(matches!(run(Request::TaskSchedules, &st).await.as_slice(), [Response::SchedulesList { .. }]));
    }

    #[tokio::test]
    async fn runs_list_all_and_by_skill() {
        let st = state();
        { let db = st.db.lock().unwrap(); db::save_task_run(&db, "some-skill", "success", "ok", "t1", "t2").unwrap(); }
        assert!(matches!(
            run(Request::RunsList { skill: None, limit: 10 }, &st).await.as_slice(),
            [Response::RunsList { runs }] if runs.len() == 1
        ));
        assert!(matches!(
            run(Request::RunsList { skill: Some("some-skill".into()), limit: 10 }, &st).await.as_slice(),
            [Response::RunsList { runs }] if runs.len() == 1
        ));
    }

    #[tokio::test]
    async fn task_create_repo_scoped_happy_then_duplicate_without_force_errors() {
        let st = state();
        let cwd = Some("/tmp/feat073-taskcreate-test".to_string());
        let created = run(
            Request::TaskCreate { name: "my-skill".into(), from_prompt: None, force: false, repo: true, cwd: cwd.clone() },
            &st,
        ).await;
        assert!(matches!(created.as_slice(), [Response::SkillCreated { .. }]));

        let dup = run(
            Request::TaskCreate { name: "my-skill".into(), from_prompt: None, force: false, repo: true, cwd },
            &st,
        ).await;
        assert!(matches!(dup.as_slice(), [Response::Error { .. }]));
    }

    #[tokio::test]
    async fn task_create_repo_scoped_requires_a_resolvable_repo() {
        let st = state();
        let r = run(
            Request::TaskCreate { name: "my-skill".into(), from_prompt: None, force: false, repo: true, cwd: None },
            &st,
        ).await;
        assert!(matches!(r.as_slice(), [Response::Error { .. }]));
    }

    #[tokio::test]
    async fn context_show_and_set_roundtrip() {
        let st = state();
        let cwd = Some("/tmp/feat073-context-test".to_string());
        assert!(matches!(
            run(Request::ContextSet { cwd: cwd.clone(), content: "some repo context".into() }, &st).await.as_slice(),
            [Response::Ok { .. }]
        ));
        match run(Request::ContextShow { cwd }, &st).await.as_slice() {
            [Response::Context { repo_key: Some(_), content: Some(c) }] => assert_eq!(c, "some repo context"),
            other => panic!("expected Context with content, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn context_show_without_a_cwd_has_no_key() {
        let st = state();
        assert!(matches!(
            run(Request::ContextShow { cwd: None }, &st).await.as_slice(),
            [Response::Context { repo_key: None, content: None }]
        ));
    }
}
