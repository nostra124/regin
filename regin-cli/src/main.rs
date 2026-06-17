use std::io::{self, BufRead, Write};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use regin_core::{
    config,
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

    /// Check if the daemon (regind) is running.
    Ping,
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
        },
        Commands::Ping => cmd_ping().await,
    }
}

// ---------------------------------------------------------------------------
// Daemon auto-start
// ---------------------------------------------------------------------------

async fn ensure_daemon() -> Result<()> {
    let sock = config::socket_path()?;
    if UnixStream::connect(&sock).await.is_ok() {
        return Ok(());
    }
    eprintln!("Starting regind...");
    let regind = std::env::current_exe()?
        .parent()
        .map(|p| p.join("regind"))
        .unwrap_or_else(|| "regind".into());
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
    match rpc(&Request::SkillList).await? {
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
    match rpc(&Request::SkillShow { name: name.into() }).await? {
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
    let unit_dir = config::user_systemd_dir()?;
    let unit_path = config::regind_service_path()?;

    if enable {
        let regind = std::env::current_exe()?
            .parent()
            .map(|p| p.join("regind"))
            .unwrap_or_else(|| "regind".into());
        let regind_str = if regind.exists() {
            regind.to_string_lossy().to_string()
        } else {
            which_cmd("regind").unwrap_or_else(|| "/usr/bin/regind".into())
        };
        std::fs::create_dir_all(&unit_dir)?;
        std::fs::write(&unit_path, config::regind_service_unit(&regind_str))?;
        println_color(&format!("  Wrote {}", unit_path.display()), Color::DarkGrey);
        let _ = Command::new("loginctl").args(["enable-linger"]).status();
        let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
        let _ = Command::new("systemctl").args(["--user", "enable", "--now", "regind"]).status();
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

async fn cmd_ping() -> Result<()> {
    match rpc(&Request::Ping).await? {
        Response::Pong => println_color("regind is running ✓", Color::Green),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}
