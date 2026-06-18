use std::io::{self, BufRead, Write};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use regin_core::{
    config, db,
    protocol::{Request, Response},
    types::ChatMessage,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

/// Regin — AI-powered Linux server administration agent.
///
/// Regin runs as a user daemon (regind) that provides an LLM-backed agent
/// with tool access (bash, file I/O, web search) for server monitoring,
/// auditing, and operations tasks.
///
/// The daemon starts automatically on first use. All configuration is stored
/// in SQLite — no config files needed.
///
/// Quick start:
///   regin config set nanogpt.api_key <key>
///   regin chat
///
/// Skills are markdown-defined tasks in /usr/share/regin/skills/ (system)
/// and ~/.config/regin/skills/ (user). User skills override system skills.
#[derive(Parser)]
#[command(
    name = "regin",
    version,
    about = "AI-powered Linux server administration agent",
    long_about = "Regin — AI-powered Linux server administration agent.\n\n\
        Named after the dwarf smith from the Völsunga saga.\n\n\
        Regin provides an LLM-backed agent with tool access (shell, file I/O,\n\
        web search) for server monitoring, auditing, and operations.\n\n\
        The daemon (regind) starts automatically. All configuration is stored\n\
        in SQLite. No config files needed.\n\n\
        GETTING STARTED:\n\
        \x20 regin config set nanogpt.api_key YOUR_KEY\n\
        \x20 regin chat\n\n\
        See regin(1) and regind(1) for full documentation.",
    after_help = "EXAMPLES:\n\
        \x20 regin chat                              Interactive agent session\n\
        \x20 regin task list                          List available tasks\n\
        \x20 regin task exec security-audit            Run a task once\n\
        \x20 regin task exec disk-usage daily          Run and schedule daily\n\
        \x20 regin memory save fact 'Server runs Ubuntu 24.04'\n\
        \x20 regin config list                        Show all settings\n\n\
        ENVIRONMENT:\n\
        \x20 XDG_RUNTIME_DIR    Socket location (default: $XDG_RUNTIME_DIR/regin/)\n\
        \x20 RUST_LOG           Log level for diagnostics (e.g. debug, info)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start an interactive chat session with the agent.
    ///
    /// The agent has access to tools: bash, file read/write/edit, and web
    /// search. It loads context from ~/.config/regin/context.md and
    /// .repo/regin/context.md (if present in cwd), plus all saved memories.
    ///
    /// In-session commands:
    ///   /new       Start a new conversation
    ///   /history   List previous conversations
    ///   /quit      Exit the session
    #[command(after_help = "The agent runs commands and edits files on your behalf.\n\
        Context files and memories are loaded automatically as system prompts.")]
    Chat,

    /// Manage and execute operational tasks (skills).
    ///
    /// Tasks are markdown-defined skills that the agent executes. System
    /// tasks ship in /usr/share/regin/skills/. User tasks go in
    /// ~/.config/regin/skills/. User tasks override system tasks with
    /// the same name.
    #[command(after_help = "SKILL FORMAT:\n\
        \x20 Each skill is a directory containing skill.md (the prompt) and\n\
        \x20 optional supporting files. The first line of skill.md is the\n\
        \x20 description shown in 'task list'.")]
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },

    /// Show recent task execution history.
    ///
    /// Displays the results of past task executions, including status,
    /// timestamps, and output previews.
    Runs {
        /// Filter runs by task (skill) name
        #[arg(long, value_name = "NAME")]
        skill: Option<String>,

        /// Maximum number of runs to display
        #[arg(long, default_value_t = 20, value_name = "N")]
        limit: u32,
    },

    /// Manage agent configuration.
    ///
    /// All settings are stored in the SQLite database. No config files.
    ///
    /// Key settings:
    ///   nanogpt.api_key     NanoGPT API key (required)
    ///   nanogpt.model       LLM model (default: claude-sonnet-4-20250514)
    ///   nanogpt.base_url    API endpoint
    ///   daemon.enabled      Run regind as persistent user service (true/false)
    #[command(after_help = "EXAMPLES:\n\
        \x20 regin config set nanogpt.api_key sk-abc123\n\
        \x20 regin config set nanogpt.model gpt-4o\n\
        \x20 regin config set daemon.enabled true\n\
        \x20 regin config get nanogpt.model\n\
        \x20 regin config list")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage the agent's long-term memory.
    ///
    /// Memories persist across sessions and are included as context in
    /// every chat and task execution. Use them to teach the agent about
    /// your infrastructure, preferences, and patterns.
    #[command(after_help = "CATEGORIES:\n\
        \x20 fact        Infrastructure facts (OS, IPs, services)\n\
        \x20 preference  How you like things done\n\
        \x20 pattern     Recurring issues or solutions\n\
        \x20 project     Project-specific context\n\
        \x20 skill       Learned operational procedures\n\
        \x20 person      Team member info\n\n\
        EXAMPLES:\n\
        \x20 regin memory save fact 'Production DB is PostgreSQL 16 on db01'\n\
        \x20 regin memory save preference 'Always use apt, never snap'\n\
        \x20 regin memory list\n\
        \x20 regin memory search postgres")]
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },

    /// Manage incidents (ITIL): unplanned interruptions or degradations.
    Incident {
        #[command(subcommand)]
        action: IncidentAction,
    },

    /// Manage changes (ITIL): documented modifications to systems.
    Change {
        #[command(subcommand)]
        action: ChangeAction,
    },

    /// Manage problems (ITIL): root causes behind recurring incidents.
    Problem {
        #[command(subcommand)]
        action: ProblemAction,
    },

    /// Show or set the per-repo context (stored in regin's own DB, keyed by repo path).
    Context {
        #[command(subcommand)]
        action: ContextAction,
    },

    /// Messaging bus: send · inbox (role@cave, via the cave inbox/outbox files).
    Bus {
        #[command(subcommand)]
        action: BusAction,
    },

    /// Check if the daemon (regind) is running.
    Ping,
}

#[derive(Subcommand)]
enum BusAction {
    /// Send a message to a role@cave address (appends to the cave outbox).
    Send {
        /// Recipient address (role@cave or owner)
        to: String,
        /// Message body
        body: String,
        /// Send as a structured (typed JSON) message instead of free text
        #[arg(long)]
        structured: bool,
        /// Optional reference id (e.g. a ticket/handover ref)
        #[arg(long)]
        ref_id: Option<String>,
    },
    /// Show inbox messages bound for this agent (advances the read cursor).
    Inbox {
        /// Peek without advancing the cursor
        #[arg(long)]
        peek: bool,
    },
}

#[derive(Subcommand)]
enum ContextAction {
    /// Show the stored context for the current repo.
    Show,
    /// Set the stored context for the current repo.
    Set {
        /// Context text
        content: String,
    },
}

#[derive(Subcommand)]
enum TaskAction {
    /// List all available tasks.
    ///
    /// Shows tasks from both system (/usr/share/regin/skills/) and user
    /// (~/.config/regin/skills/) directories, with source indicated.
    List,

    /// Show details of a task: its prompt and supporting files.
    Show {
        /// Task (skill) name
        name: String,
    },

    /// Execute a task, optionally scheduling it for repeated execution.
    ///
    /// Without a schedule, the task runs once. With a schedule argument,
    /// the task runs immediately AND is scheduled for future execution.
    /// The agent has full tool access during task execution.
    #[command(after_help = "SCHEDULE VALUES:\n\
        \x20 hourly       Every hour\n\
        \x20 daily        Every 24 hours\n\
        \x20 weekly       Every 7 days\n\
        \x20 monthly      Every 30 days\n\
        \x20 'every 5m'   Every 5 minutes\n\
        \x20 'every 2h'   Every 2 hours\n\
        \x20 'every 7d'   Every 7 days")]
    Exec {
        /// Task (skill) name
        name: String,

        /// Schedule interval: hourly, daily, weekly, monthly, or 'every Xm/Xh/Xd'
        schedule: Option<String>,
    },

    /// Remove a recurring schedule for a task.
    Unschedule {
        /// Task (skill) name
        name: String,
    },

    /// List all active task schedules.
    Schedules,

    /// Create a new task (skill) in the user skills dir.
    ///
    /// Scaffolds a template skill.md, or — with --from-prompt — has the agent
    /// draft it (requires nanogpt.api_key). Refuses to overwrite an existing
    /// user skill unless --force.
    Create {
        /// Task (skill) name
        name: String,
        /// Have the agent draft the skill from this goal
        #[arg(long = "from-prompt", value_name = "GOAL")]
        from_prompt: Option<String>,
        /// Overwrite an existing skill of the same name
        #[arg(long)]
        force: bool,
        /// Open $EDITOR on the new skill.md (user skills only)
        #[arg(long)]
        edit: bool,
        /// Store in the current repo's per-repo store (XDG, keyed by repo path)
        #[arg(long)]
        repo: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// List all settings and their current values.
    List,

    /// Get the value of a setting.
    Get {
        /// Setting key (e.g. nanogpt.api_key)
        key: String,
    },

    /// Set a configuration value.
    ///
    /// Setting daemon.enabled to 'true' installs and enables a user
    /// systemd service with lingering, so regind survives logout and
    /// starts at boot.
    Set {
        /// Setting key
        key: String,
        /// New value
        value: String,
    },
}

#[derive(Subcommand)]
enum MemoryAction {
    /// List all memories, optionally filtered by category.
    List {
        /// Filter by category (e.g. fact, preference, pattern)
        #[arg(long, short, value_name = "CATEGORY")]
        category: Option<String>,
    },

    /// Search memories by content.
    Search {
        /// Search query
        query: String,
    },

    /// Save a new memory.
    Save {
        /// Category: fact, preference, pattern, project, skill, person
        category: String,
        /// Memory content
        content: String,
    },

    /// Update an existing memory by ID.
    Update {
        /// Memory ID (from 'memory list')
        id: String,
        /// New content
        content: String,
    },

    /// Delete a memory by ID.
    Delete {
        /// Memory ID
        id: String,
    },

    /// Run a Hermes reflection pass now: distil recent episodes into memories.
    Reflect,
}

#[derive(Subcommand)]
enum IncidentAction {
    /// Open a new incident.
    Open {
        /// Short title
        title: String,
        /// Severity (e.g. low, medium, high, critical)
        #[arg(long, default_value = "medium")]
        severity: String,
        /// Longer description
        #[arg(long, default_value = "")]
        desc: String,
    },
    /// List incidents, optionally filtered by status.
    List {
        /// Filter: open, investigating, resolved, closed
        #[arg(long)]
        status: Option<String>,
    },
    /// Show one incident by id.
    Show { id: String },
    /// Update an incident's status (e.g. investigating).
    Update {
        id: String,
        #[arg(long)]
        status: String,
    },
    /// Resolve an incident with a resolution note.
    Resolve { id: String, resolution: String },
    /// Close an incident.
    Close { id: String },
}

#[derive(Subcommand)]
enum ChangeAction {
    /// Record a planned change.
    Record {
        /// Short title
        title: String,
        #[arg(long, default_value = "")]
        desc: String,
        /// The incident this change remediates
        #[arg(long)]
        incident: Option<String>,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
    },
    /// List all changes.
    List,
    /// Show one change by id.
    Show { id: String },
    /// Mark a change applied.
    Apply { id: String },
    /// Close a change.
    Close { id: String },
}

#[derive(Subcommand)]
enum ProblemAction {
    /// Open a new problem.
    Open {
        title: String,
        #[arg(long, default_value = "")]
        desc: String,
    },
    /// List problems, optionally filtered by status.
    List {
        /// Filter: open, known_error, closed
        #[arg(long)]
        status: Option<String>,
    },
    /// Show one problem by id.
    Show { id: String },
    /// Link an incident to a problem.
    Link { problem_id: String, incident_id: String },
    /// Promote a problem to a known error with a root cause.
    KnownError { id: String, root_cause: String },
    /// Close a problem.
    Close { id: String },
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Chat => cmd_chat().await,
        Commands::Task { action } => match action {
            TaskAction::List => cmd_task_list().await,
            TaskAction::Show { name } => cmd_task_show(&name).await,
            TaskAction::Exec { name, schedule } => cmd_task_exec(&name, schedule.as_deref()).await,
            TaskAction::Unschedule { name } => cmd_task_unschedule(&name).await,
            TaskAction::Schedules => cmd_task_schedules().await,
            TaskAction::Create { name, from_prompt, force, edit, repo } => {
                cmd_task_create(&name, from_prompt, force, edit, repo).await
            }
        },
        Commands::Runs { skill, limit } => cmd_runs(skill.as_deref(), limit).await,
        Commands::Config { action } => match action {
            ConfigAction::List => cmd_config_list().await,
            ConfigAction::Get { key } => cmd_config_get(&key).await,
            ConfigAction::Set { key, value } => cmd_config_set(&key, &value).await,
        },
        Commands::Memory { action } => match action {
            MemoryAction::List { category } => cmd_memory_list(category.as_deref()).await,
            MemoryAction::Search { query } => cmd_memory_search(&query).await,
            MemoryAction::Save { category, content } => cmd_memory_save(&category, &content).await,
            MemoryAction::Update { id, content } => cmd_memory_update(&id, &content).await,
            MemoryAction::Delete { id } => cmd_memory_delete(&id).await,
            MemoryAction::Reflect => cmd_memory_reflect().await,
        },
        Commands::Incident { action } => match action {
            IncidentAction::Open { title, severity, desc } => {
                cmd_ok(Request::IncidentOpen { title, description: desc, severity }).await
            }
            IncidentAction::List { status } => cmd_incidents(Request::IncidentList { status }).await,
            IncidentAction::Show { id } => cmd_incidents(Request::IncidentShow { id }).await,
            IncidentAction::Update { id, status } => cmd_ok(Request::IncidentUpdate { id, status }).await,
            IncidentAction::Resolve { id, resolution } => cmd_ok(Request::IncidentResolve { id, resolution }).await,
            IncidentAction::Close { id } => cmd_ok(Request::IncidentClose { id }).await,
        },
        Commands::Change { action } => match action {
            ChangeAction::Record { title, desc, incident, before, after } => {
                cmd_ok(Request::ChangeRecord { title, description: desc, incident_id: incident, before, after }).await
            }
            ChangeAction::List => cmd_changes(Request::ChangeList).await,
            ChangeAction::Show { id } => cmd_changes(Request::ChangeShow { id }).await,
            ChangeAction::Apply { id } => cmd_ok(Request::ChangeApply { id }).await,
            ChangeAction::Close { id } => cmd_ok(Request::ChangeClose { id }).await,
        },
        Commands::Problem { action } => match action {
            ProblemAction::Open { title, desc } => cmd_ok(Request::ProblemOpen { title, description: desc }).await,
            ProblemAction::List { status } => cmd_problems(Request::ProblemList { status }).await,
            ProblemAction::Show { id } => cmd_problems(Request::ProblemShow { id }).await,
            ProblemAction::Link { problem_id, incident_id } => cmd_ok(Request::ProblemLink { problem_id, incident_id }).await,
            ProblemAction::KnownError { id, root_cause } => cmd_ok(Request::ProblemKnownError { id, root_cause }).await,
            ProblemAction::Close { id } => cmd_ok(Request::ProblemClose { id }).await,
        },
        Commands::Context { action } => match action {
            ContextAction::Show => cmd_context_show().await,
            ContextAction::Set { content } => {
                cmd_ok(Request::ContextSet { cwd: Some(cwd_string()), content }).await
            }
        },
        Commands::Bus { action } => match action {
            BusAction::Send { to, body, structured, ref_id } => cmd_bus_send(&to, &body, structured, ref_id.as_deref()),
            BusAction::Inbox { peek } => cmd_bus_inbox(peek),
        },
        Commands::Ping => cmd_ping().await,
    }
}

// ---------------------------------------------------------------------------
// Bus (FEAT-010): file-based cave inbox/outbox — no daemon round-trip needed.

fn cmd_bus_send(to: &str, body: &str, structured: bool, ref_id: Option<&str>) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED, KIND_UNSTRUCTURED};
    let client = BusClient::from_env()?;
    let kind = if structured { KIND_STRUCTURED } else { KIND_UNSTRUCTURED };
    client.send(to, kind, body, ref_id)?;
    println!("regin: sent {} -> {to}", client.address());
    Ok(())
}

fn cmd_bus_inbox(peek: bool) -> Result<()> {
    use regin_core::bus::BusClient;
    let client = BusClient::from_env()?;
    let msgs = client.inbox(!peek)?;
    if msgs.is_empty() {
        println!("regin: inbox empty ({})", client.address());
        return Ok(());
    }
    for m in &msgs {
        let r = m.ref_id.as_deref().map(|r| format!(" [{r}]")).unwrap_or_default();
        println!("{} → {} ({}){}: {}", m.sender, m.recipient, m.kind, r, m.body);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Daemon auto-start
// ---------------------------------------------------------------------------

async fn ensure_daemon() -> Result<()> {
    let sock = config::socket_path()?;
    if UnixStream::connect(&sock).await.is_ok() {
        return Ok(());
    }

    // BUG-001: prefer registering the persistent systemd *user* service so regind
    // survives logout/reboot, instead of a loose transient process. Honour an
    // opt-out (daemon.auto_register = false) and fall back to a transient spawn
    // when systemd-user is unavailable (e.g. minimal containers).
    let auto_register = read_local_setting("daemon.auto_register")
        .map(|v| v != "false")
        .unwrap_or(true);

    if auto_register && systemd_user_available() {
        eprintln!("Registering regind as a user service...");
        if install_regind_service().is_ok() {
            let _ = set_local_setting("daemon.enabled", "true");
            for _ in 0..50 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if UnixStream::connect(&sock).await.is_ok() {
                    return Ok(());
                }
            }
        }
        // fall through to a transient spawn if the service did not come up
    }

    eprintln!("Starting regind...");
    let regind = regind_bin();
    if regind.exists() {
        let _ = Command::new(&regind).spawn();
    } else {
        let _ = Command::new("regind").spawn();
    }
    for _ in 0..30 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if UnixStream::connect(&sock).await.is_ok() {
            return Ok(());
        }
    }
    Err(anyhow!("Failed to start regind. Run it manually or check logs."))
}

/// Path to the bundled `regind` binary next to this executable.
fn regind_bin() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("regind")))
        .unwrap_or_else(|| "regind".into())
}

/// Whether a systemd *user* manager is reachable (so we can install a service).
fn systemd_user_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Read a setting directly from the SQLite store (used before the daemon is up).
fn read_local_setting(key: &str) -> Option<String> {
    let path = config::db_path().ok()?;
    let conn = db::init_db(&path).ok()?;
    db::setting_get(&conn, key).ok()
}

/// Write a setting directly to the SQLite store (used before the daemon is up).
fn set_local_setting(key: &str, value: &str) -> Result<()> {
    let path = config::db_path()?;
    let conn = db::init_db(&path)?;
    db::setting_set(&conn, key, value)?;
    Ok(())
}

/// Install + enable the regind systemd user service with lingering, so it
/// survives logout and starts at boot.
fn install_regind_service() -> Result<()> {
    let unit_dir = config::user_systemd_dir()?;
    let unit_path = config::regind_service_path()?;
    let regind = regind_bin();
    let regind_str = if regind.exists() {
        regind.to_string_lossy().to_string()
    } else {
        which_cmd("regind").unwrap_or_else(|| "/usr/bin/regind".into())
    };
    std::fs::create_dir_all(&unit_dir)?;
    std::fs::write(&unit_path, config::regind_service_unit(&regind_str))?;
    let _ = Command::new("loginctl").args(["enable-linger"]).status();
    let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
    let _ = Command::new("systemctl").args(["--user", "enable", "--now", "regind"]).status();
    Ok(())
}

// ---------------------------------------------------------------------------
// Socket helpers
// ---------------------------------------------------------------------------

async fn connect_daemon() -> Result<(
    tokio::io::WriteHalf<UnixStream>,
    BufReader<tokio::io::ReadHalf<UnixStream>>,
)> {
    ensure_daemon().await?;
    let sock = config::socket_path()?;
    let stream = UnixStream::connect(&sock).await
        .with_context(|| format!("Cannot connect to regind at {}", sock.display()))?;
    let (r, w) = tokio::io::split(stream);
    Ok((w, BufReader::new(r)))
}

async fn send_req(w: &mut tokio::io::WriteHalf<UnixStream>, req: &Request) -> Result<()> {
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    Ok(())
}

async fn read_resp(r: &mut BufReader<tokio::io::ReadHalf<UnixStream>>) -> Result<Response> {
    let mut line = String::new();
    let n = r.read_line(&mut line).await?;
    if n == 0 { return Err(anyhow!("Connection closed")); }
    Ok(serde_json::from_str(&line)?)
}

async fn rpc(req: &Request) -> Result<Response> {
    let (mut w, mut r) = connect_daemon().await?;
    send_req(&mut w, req).await?;
    let resp = read_resp(&mut r).await?;
    if let Response::Error { ref message } = resp {
        return Err(anyhow!("{message}"));
    }
    Ok(resp)
}

fn cwd_string() -> String {
    std::env::current_dir().unwrap_or_default().to_string_lossy().to_string()
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

fn print_color(text: &str, color: Color) {
    let mut out = io::stdout();
    let _ = crossterm::execute!(out, SetForegroundColor(color));
    let _ = write!(out, "{text}");
    let _ = crossterm::execute!(out, ResetColor);
}

fn println_color(text: &str, color: Color) {
    print_color(text, color);
    println!();
}

/// Handle streamed responses (text chunks + tool activity) until StreamDone.
/// Returns (final_text, conversation_id).
async fn consume_stream(
    r: &mut BufReader<tokio::io::ReadHalf<UnixStream>>,
) -> Result<(String, String)> {
    let mut full = String::new();
    let mut conv_id = String::new();

    loop {
        let resp = read_resp(r).await?;
        match resp {
            Response::StreamChunk { token } => {
                print_color(&token, Color::Green);
                io::stdout().flush()?;
                full.push_str(&token);
            }
            Response::ToolCallEvent { name, arguments } => {
                let args_preview: String = arguments.chars().take(120).collect();
                println!();
                print_color(&format!("▶ {name}"), Color::Magenta);
                println_color(&format!(" {args_preview}"), Color::DarkGrey);
            }
            Response::ToolResultEvent { name, success, output } => {
                let icon = if success { "✓" } else { "✗" };
                let color = if success { Color::Green } else { Color::Red };
                print_color(&format!("  {icon} {name}"), color);
                // Show first few lines of output
                let preview: String = output.lines().take(5).collect::<Vec<_>>().join("\n    ");
                if !preview.is_empty() {
                    println_color(&format!("\n    {preview}"), Color::DarkGrey);
                } else {
                    println!();
                }
            }
            Response::StreamDone { conversation_id } => {
                conv_id = conversation_id;
                break;
            }
            Response::Error { message } => {
                println_color(&format!("\n[error: {message}]"), Color::Red);
                break;
            }
            _ => break,
        }
    }
    Ok((full, conv_id))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn cmd_chat() -> Result<()> {
    let (mut w, mut r) = connect_daemon().await?;

    send_req(&mut w, &Request::ChatNew).await?;
    let mut conv_id = match read_resp(&mut r).await? {
        Response::ChatNew { conversation_id } => conversation_id,
        resp => return Err(anyhow!("Unexpected: {resp:?}")),
    };

    println_color("regin — Linux server administration agent", Color::Yellow);
    println_color("Commands: /new  /history  /quit", Color::DarkGrey);
    println!();

    let mut history: Vec<ChatMessage> = Vec::new();
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        print_color("you> ", Color::Cyan);
        io::stdout().flush()?;

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        match trimmed {
            "/quit" | "/exit" => break,
            "/new" => {
                send_req(&mut w, &Request::ChatNew).await?;
                conv_id = match read_resp(&mut r).await? {
                    Response::ChatNew { conversation_id } => conversation_id,
                    _ => continue,
                };
                history.clear();
                println_color("— new conversation —", Color::Yellow);
                println!();
                continue;
            }
            "/history" => {
                send_req(&mut w, &Request::ChatHistory).await?;
                if let Response::ChatHistory { conversations } = read_resp(&mut r).await? {
                    if conversations.is_empty() {
                        println_color("No conversations yet.", Color::Yellow);
                    } else {
                        for c in conversations.iter().take(20) {
                            println!("  {} | {} | {}", &c.id[..c.id.len().min(8)], c.updated_at, c.title);
                        }
                    }
                }
                println!();
                continue;
            }
            _ => {}
        }

        history.push(ChatMessage::user(trimmed));
        send_req(&mut w, &Request::ChatSend {
            conversation_id: conv_id.clone(),
            messages: history.clone(),
            cwd: Some(cwd_string()),
        }).await?;

        print_color("regin> ", Color::Green);
        io::stdout().flush()?;

        let (full, new_conv) = consume_stream(&mut r).await?;
        if !new_conv.is_empty() { conv_id = new_conv; }
        println!();
        println!();

        if !full.is_empty() {
            history.push(ChatMessage::assistant(&full));
        }
    }

    println_color("Goodbye.", Color::Yellow);
    Ok(())
}

async fn cmd_task_list() -> Result<()> {
    match rpc(&Request::SkillList { cwd: Some(cwd_string()) }).await? {
        Response::SkillList { skills } => {
            if skills.is_empty() {
                println!("No tasks found.");
                println_color("  Add skills to ~/.config/regin/skills/ or /usr/share/regin/skills/", Color::DarkGrey);
                return Ok(());
            }
            println_color(&format!("Tasks ({}):", skills.len()), Color::Yellow);
            for s in &skills {
                print_color(&format!("  {:<20}", s.name), Color::Cyan);
                print_color(&format!("[{:<6}] ", s.source), Color::DarkGrey);
                println!("{}", s.description);
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_show(name: &str) -> Result<()> {
    match rpc(&Request::SkillShow { name: name.into(), cwd: Some(cwd_string()) }).await? {
        Response::SkillDetail { name, description, prompt, files } => {
            println_color(&format!("Task: {name}"), Color::Cyan);
            println!("  {description}");
            println!();
            println_color("— prompt —", Color::Yellow);
            println!("{prompt}");
            if !files.is_empty() {
                println_color(&format!("Supporting files ({}):", files.len()), Color::Yellow);
                for f in &files { println!("  • {f}"); }
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_exec(name: &str, schedule: Option<&str>) -> Result<()> {
    let (mut w, mut r) = connect_daemon().await?;

    if let Some(interval) = schedule {
        send_req(&mut w, &Request::TaskSchedule {
            skill: name.into(), interval: interval.into(),
        }).await?;
        match read_resp(&mut r).await? {
            Response::Ok { message } => println_color(&format!("✓ {message}"), Color::Yellow),
            Response::Error { message } => return Err(anyhow!("Schedule error: {message}")),
            _ => {}
        }
    }

    println_color(&format!("Running '{name}'…"), Color::Yellow);
    send_req(&mut w, &Request::TaskExec {
        skill: name.into(),
        cwd: Some(cwd_string()),
    }).await?;

    // Consume tool events until TaskResult
    loop {
        let resp = read_resp(&mut r).await?;
        match resp {
            Response::ToolCallEvent { name, arguments } => {
                let preview: String = arguments.chars().take(120).collect();
                print_color(&format!("▶ {name}"), Color::Magenta);
                println_color(&format!(" {preview}"), Color::DarkGrey);
            }
            Response::ToolResultEvent { name, success, output } => {
                let icon = if success { "✓" } else { "✗" };
                let color = if success { Color::Green } else { Color::Red };
                print_color(&format!("  {icon} {name}"), color);
                let preview: String = output.lines().take(3).collect::<Vec<_>>().join("\n    ");
                if !preview.is_empty() { println_color(&format!("\n    {preview}"), Color::DarkGrey); }
                else { println!(); }
            }
            Response::StreamChunk { token } => {
                print_color(&token, Color::Green);
                io::stdout().flush()?;
            }
            Response::StreamDone { .. } => {}
            Response::TaskResult { run } => {
                println!();
                let color = if run.status == "success" { Color::Green } else { Color::Red };
                println_color(&format!("Status: {}", run.status), color);
                println!();
                println!("{}", run.output);
                break;
            }
            Response::Error { message } => {
                println_color(&format!("Error: {message}"), Color::Red);
                break;
            }
            _ => break,
        }
    }
    Ok(())
}

async fn cmd_task_unschedule(name: &str) -> Result<()> {
    match rpc(&Request::TaskUnschedule { skill: name.into() }).await? {
        Response::Ok { message } => println_color(&message, Color::Yellow),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_schedules() -> Result<()> {
    match rpc(&Request::TaskSchedules).await? {
        Response::SchedulesList { schedules } => {
            if schedules.is_empty() {
                println!("No active schedules.");
                return Ok(());
            }
            println_color(&format!("Schedules ({}):", schedules.len()), Color::Yellow);
            for s in &schedules {
                print_color(&format!("  {:<20}", s.skill), Color::Cyan);
                print!("  {:<10}  next: {}", s.interval, s.next_run);
                if let Some(ref last) = s.last_run { print!("  last: {last}"); }
                println!();
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_runs(skill: Option<&str>, limit: u32) -> Result<()> {
    match rpc(&Request::RunsList { skill: skill.map(String::from), limit }).await? {
        Response::RunsList { runs } => {
            if runs.is_empty() {
                println!("No task runs found.");
                return Ok(());
            }
            println_color(&format!("Task runs ({}):", runs.len()), Color::Yellow);
            for r in &runs {
                let color = if r.status == "success" { Color::Green } else { Color::Red };
                print!("  {} | {:<20} | ", r.started_at, r.skill_name);
                println_color(&r.status, color);
                if let Some(first) = r.output.lines().next() {
                    let preview: String = first.chars().take(100).collect();
                    println_color(&format!("    {preview}"), Color::DarkGrey);
                }
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_config_list() -> Result<()> {
    match rpc(&Request::ConfigList).await? {
        Response::ConfigEntries { entries } => {
            println_color("Settings:", Color::Yellow);
            for (key, value) in &entries {
                let display = if key.contains("api_key") && value.len() > 8 {
                    format!("{}…{}", &value[..4], &value[value.len()-4..])
                } else if key.contains("api_key") && !value.is_empty() {
                    "****".into()
                } else {
                    value.clone()
                };
                print_color(&format!("  {key:<25}"), Color::Cyan);
                println!("{display}");
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_config_get(key: &str) -> Result<()> {
    match rpc(&Request::ConfigGet { key: key.into() }).await? {
        Response::ConfigValue { key, value } => {
            if key.contains("api_key") && value.len() > 8 {
                println!("{}…{}", &value[..4], &value[value.len()-4..]);
            } else {
                println!("{value}");
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_config_set(key: &str, value: &str) -> Result<()> {
    if key == "daemon.enabled" {
        handle_daemon_enabled(value).await?;
    }
    match rpc(&Request::ConfigSet { key: key.into(), value: value.into() }).await? {
        Response::Ok { message } => println_color(&format!("✓ {message}"), Color::Green),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn handle_daemon_enabled(value: &str) -> Result<()> {
    let enable = matches!(value, "true" | "1" | "yes");
    let unit_path = config::regind_service_path()?;

    if enable {
        install_regind_service()?;
        println_color(&format!("  Wrote {}", unit_path.display()), Color::DarkGrey);
        println_color("  ✓ regind enabled as user service (survives logout)", Color::Green);
    } else {
        let _ = Command::new("systemctl").args(["--user", "disable", "--now", "regind"]).status();
        if unit_path.exists() { std::fs::remove_file(&unit_path)?; }
        let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
        println_color("  ✓ regind disabled as user service", Color::Green);
    }
    Ok(())
}

fn which_cmd(name: &str) -> Option<String> {
    Command::new("which").arg(name).output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn cmd_memory_list(category: Option<&str>) -> Result<()> {
    match rpc(&Request::MemoryList { category: category.map(String::from) }).await? {
        Response::MemoryList { memories } => {
            if memories.is_empty() {
                println!("No memories stored.");
                println_color("  Save one with: regin memory save <category> '<content>'", Color::DarkGrey);
                return Ok(());
            }
            println_color(&format!("Memories ({}):", memories.len()), Color::Yellow);
            for m in &memories {
                print_color(&format!("  {}", &m.id[..m.id.len().min(8)]), Color::DarkGrey);
                print_color(&format!("  [{:<10}] ", m.category), Color::Cyan);
                println!("{}", m.content);
                if m.source == "reflection" {
                    println_color(
                        &format!("            ⟳ reflection · strength {}", m.strength),
                        Color::DarkGrey,
                    );
                }
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_search(query: &str) -> Result<()> {
    match rpc(&Request::MemorySearch { query: query.into() }).await? {
        Response::MemoryList { memories } => {
            if memories.is_empty() {
                println!("No matching memories.");
                return Ok(());
            }
            for m in &memories {
                print_color(&format!("  {}", &m.id[..m.id.len().min(8)]), Color::DarkGrey);
                print_color(&format!("  [{:<10}] ", m.category), Color::Cyan);
                println!("{}", m.content);
            }
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_save(category: &str, content: &str) -> Result<()> {
    match rpc(&Request::MemorySave { category: category.into(), content: content.into() }).await? {
        Response::Ok { message } => println_color(&format!("✓ {message}"), Color::Green),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_update(id: &str, content: &str) -> Result<()> {
    match rpc(&Request::MemoryUpdate { id: id.into(), content: content.into() }).await? {
        Response::Ok { message } => println_color(&format!("✓ {message}"), Color::Green),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_delete(id: &str) -> Result<()> {
    match rpc(&Request::MemoryDelete { id: id.into() }).await? {
        Response::Ok { message } => println_color(&format!("✓ {message}"), Color::Green),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_create(
    name: &str,
    from_prompt: Option<String>,
    force: bool,
    edit: bool,
    repo: bool,
) -> Result<()> {
    let cwd = repo.then(cwd_string);
    match rpc(&Request::TaskCreate { name: name.into(), from_prompt, force, repo, cwd }).await? {
        Response::SkillCreated { path, shadows_system } => {
            println_color(&format!("✓ Created skill '{name}' at {path}"), Color::Green);
            if shadows_system {
                println_color(
                    &format!("  note: this user skill shadows a system skill named '{name}'"),
                    Color::Yellow,
                );
            }
            if edit && !repo {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
                std::process::Command::new(editor)
                    .arg(&path)
                    .status()
                    .map_err(|e| anyhow!("Failed to open editor: {e}"))?;
            } else if !repo {
                println_color(&format!("  edit it: $EDITOR {path}"), Color::DarkGrey);
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_context_show() -> Result<()> {
    match rpc(&Request::ContextShow { cwd: Some(cwd_string()) }).await? {
        Response::Context { repo_key, content } => {
            match repo_key {
                Some(k) => println_color(&format!("repo: {k}"), Color::DarkGrey),
                None => println!("(no repo resolved for the current directory)"),
            }
            match content {
                Some(c) => println!("{c}"),
                None => println_color("  (no context stored — set one with: regin context set '<text>')", Color::DarkGrey),
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_reflect() -> Result<()> {
    match rpc(&Request::MemoryReflect).await? {
        Response::ReflectStats { episodes, reinforced, created, decayed } => {
            println_color(
                &format!(
                    "✓ Reflection: {episodes} episodes → {reinforced} reinforced, {created} new, {decayed} decayed"
                ),
                Color::Green,
            );
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_ping() -> Result<()> {
    match rpc(&Request::Ping).await? {
        Response::Pong => println_color("regind is running ✓", Color::Green),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

// --- ITIL command helpers ---

fn sid(id: &str) -> &str {
    &id[..id.len().min(8)]
}

/// Run a request that returns a simple Ok/Error acknowledgement.
async fn cmd_ok(req: Request) -> Result<()> {
    match rpc(&req).await? {
        Response::Ok { message } => println_color(&format!("✓ {message}"), Color::Green),
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_incidents(req: Request) -> Result<()> {
    match rpc(&req).await? {
        Response::Incidents { incidents } => {
            if incidents.is_empty() {
                println!("No incidents.");
                return Ok(());
            }
            for i in &incidents {
                print_color(&format!("  {}", sid(&i.id)), Color::DarkGrey);
                print_color(&format!("  [{:<13}] ", i.status), Color::Cyan);
                print_color(&format!("{:<8} ", i.severity), Color::Yellow);
                println!("{}", i.title);
                if !i.description.is_empty() {
                    println!("            {}", i.description);
                }
                if let Some(p) = &i.problem_id {
                    println_color(&format!("            problem: {}", sid(p)), Color::DarkGrey);
                }
                if let Some(r) = &i.resolution {
                    println_color(&format!("            resolution: {r}"), Color::Green);
                }
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_changes(req: Request) -> Result<()> {
    match rpc(&req).await? {
        Response::Changes { changes } => {
            if changes.is_empty() {
                println!("No changes.");
                return Ok(());
            }
            for c in &changes {
                print_color(&format!("  {}", sid(&c.id)), Color::DarkGrey);
                print_color(&format!("  [{:<8}] ", c.status), Color::Cyan);
                println!("{}", c.title);
                if let Some(inc) = &c.incident_id {
                    println_color(&format!("            incident: {}", sid(inc)), Color::DarkGrey);
                }
                if c.before.is_some() || c.after.is_some() {
                    println!(
                        "            {} -> {}",
                        c.before.as_deref().unwrap_or("?"),
                        c.after.as_deref().unwrap_or("?")
                    );
                }
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_problems(req: Request) -> Result<()> {
    match rpc(&req).await? {
        Response::Problems { problems } => {
            if problems.is_empty() {
                println!("No problems.");
                return Ok(());
            }
            for p in &problems {
                print_color(&format!("  {}", sid(&p.id)), Color::DarkGrey);
                print_color(&format!("  [{:<11}] ", p.status), Color::Cyan);
                println!("{}", p.title);
                if let Some(rc) = &p.root_cause {
                    println_color(&format!("            root cause: {rc}"), Color::Yellow);
                }
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}
