use std::io::{self, BufRead, Write};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
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
    /// search. It loads per-repo context, skills, and memories from regin's
    /// own store (SQLite under the XDG data dir), keyed by the current repo's
    /// filesystem path — manage it with `regin context`. A global user context
    /// (~/.config/regin/context.md), if present, applies everywhere; all saved
    /// memories are injected too.
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

    /// Inspect the desired (to-be) state per domain (FEAT-033).
    Desired {
        #[command(subcommand)]
        action: DesiredAction,
    },

    /// Show CSI metrics: KPIs + the cost-vs-reliability objective (FEAT-050).
    Metrics {
        /// Window in days (default 30).
        #[arg(long)]
        days: Option<u32>,
    },

    /// Inspect notice filters that suppress known noise before the LLM (FEAT-052).
    Filters {
        #[command(subcommand)]
        action: FiltersAction,
    },

    /// Show regin's effective operating mode: org (supervisor) vs standalone (FEAT-041).
    Mode,

    /// Show the adaptive autonomy posture and the evidence behind it (FEAT-040).
    Posture,

    /// Show the login greeting: health + parked items needing a decision (FEAT-043).
    Greeting,

    /// Active push for critical items (opt-in, off by default) (FEAT-044).
    Push {
        #[command(subcommand)]
        action: PushAction,
    },

    /// List active derived (promoted) deterministic checks (FEAT-051).
    Checks,

    /// Run the periodic CSI self-audit now and file its findings (FEAT-055).
    Audit,

    /// Generate man pages from the CLI into a directory (FEAT-019; used by packaging).
    #[command(hide = true)]
    GenMan { dir: String },

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

    /// Show the active role persona (REGIN_PERSONA) and its capability ceiling.
    Persona,

    /// Chair a meeting: collect inbox reports → compile minutes → emit to dvalin.
    Meeting {
        #[command(subcommand)]
        action: MeetingAction,
    },

    /// Run the individual planning cycle (aggregate When/Which → plan; emit upward).
    Plan {
        /// Cadence: weekly | monthly | yearly
        #[arg(long, default_value = "weekly")]
        cadence: String,
        /// A required capability/skill package this agent needs (repeatable)
        #[arg(long = "need")]
        needs: Vec<String>,
        /// Emit upward signals (priority ask + capability gap) over the bus
        #[arg(long)]
        emit: bool,
        /// Process/project owner address (priority ask). Else REGIN_PROCESS_OWNER.
        #[arg(long)]
        owner: Option<String>,
        /// CAO address (capability gaps). Else REGIN_CAO, else cao@hq.
        #[arg(long)]
        cao: Option<String>,
    },

    /// Foreman mode: drain the inbox, run cave-tasks on local workers, hand over.
    Foreman {
        #[command(subcommand)]
        action: ForemanAction,
    },

    /// Skill packages: install a regin-*-skills package · list installed.
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },

    /// Deputy mode: assign · show · brief · activate · handback (continuity).
    Deputy {
        #[command(subcommand)]
        action: DeputyAction,
    },

    /// Check if the daemon (regind) is running.
    Ping,
}

#[derive(Subcommand)]
enum DeputyAction {
    /// Assign this regin as the deputy of a role held by a primary
    Assign { role: String, primary: String },
    /// Show the current deputy assignment + state + brief
    Show,
    /// Set the standing continuity brief (maintained by the primary)
    Brief { text: String },
    /// Activate on failover (requires --confirmed by the supervisor)
    Activate {
        /// Supervisor confirmation of the failover
        #[arg(long)]
        confirmed: bool,
    },
    /// Hand back to the primary when it returns
    Handback,
}

#[derive(Subcommand)]
enum SkillAction {
    /// Install a skill package directory (regin-*-skills) into the user skills store.
    Install {
        /// Path to the package directory (contains package.toml + skills/)
        dir: std::path::PathBuf,
    },
    /// List installed skill packages.
    Packages,
}

#[derive(Subcommand)]
enum MeetingAction {
    /// Chair a meeting: compile minutes from inbox reports and emit them.
    Chair {
        /// Meeting name (e.g. board)
        name: String,
        /// An agenda item (repeatable; defaults to a standard agenda)
        #[arg(long = "agenda")]
        agenda: Vec<String>,
        /// Address to send the minutes to (else REGIN_MEETING_RECORDER, else dvalin@hq)
        #[arg(long)]
        to: Option<String>,
    },
}

#[derive(Subcommand)]
enum ForemanAction {
    /// Drain the inbox once: handle each cave-task and post handovers.
    RunOnce {
        /// Plan only — show what would run without spawning workers.
        #[arg(long)]
        dry_run: bool,
    },
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
    /// Block an incident on a workaround while its problem awaits a fix.
    Block { id: String, workaround: String },
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
        /// The problem this change resolves
        #[arg(long)]
        problem: Option<String>,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
    },
    /// List all changes.
    List,
    /// Show one change by id.
    Show { id: String },
    /// Move a change to pending_approval (awaiting a decision).
    RequestApproval { id: String },
    /// Approve a pending change, recording the approver.
    Approve {
        id: String,
        /// Who approved it
        #[arg(long, default_value = "operator")]
        by: String,
    },
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
    /// Add a root-cause hypothesis to a problem.
    HypothesisAdd { problem_id: String, text: String },
    /// List a problem's hypotheses.
    HypothesisList { problem_id: String },
    /// Set a hypothesis's status: created|validating|confirmed|rejected.
    HypothesisStatus { id: String, status: String },
    /// Escalate a problem to dvalin as a BUG/FEAT (structured bus message).
    Escalate {
        /// Problem id
        id: String,
        /// Ticket kind to request
        #[arg(long = "as", default_value = "bug")]
        kind: String,
        /// dvalin exec address to escalate to (else REGIN_ESCALATION_TO, else cio@hq)
        #[arg(long)]
        to: Option<String>,
    },
    /// Close a problem.
    Close { id: String },
}

#[derive(Subcommand)]
enum PushAction {
    /// Send a test notification over the configured channel.
    Test,
}

#[derive(Subcommand)]
enum FiltersAction {
    /// List loaded notice-filter rules (system + user).
    List,
    /// Test whether an observation would be filtered before reaching the LLM.
    Test { domain: String, text: String },
}

#[derive(Subcommand)]
enum DesiredAction {
    /// List loaded desired-state domains (system + user), flagging conflicts.
    List,
    /// Show one domain's intent + assertions.
    Show { domain: String },
    /// Re-check targets, opening a problem for any contradictory domain.
    Check,
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
        Commands::GenMan { dir } => cmd_gen_man(&dir),
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
            IncidentAction::Block { id, workaround } => cmd_ok(Request::IncidentBlock { id, workaround }).await,
            IncidentAction::Close { id } => cmd_ok(Request::IncidentClose { id }).await,
        },
        Commands::Change { action } => match action {
            ChangeAction::Record { title, desc, incident, problem, before, after } => {
                cmd_ok(Request::ChangeRecord { title, description: desc, incident_id: incident, problem_id: problem, before, after }).await
            }
            ChangeAction::List => cmd_changes(Request::ChangeList).await,
            ChangeAction::Show { id } => cmd_changes(Request::ChangeShow { id }).await,
            ChangeAction::RequestApproval { id } => cmd_ok(Request::ChangeRequestApproval { id }).await,
            ChangeAction::Approve { id, by } => cmd_ok(Request::ChangeApprove { id, approved_by: by }).await,
            ChangeAction::Apply { id } => cmd_ok(Request::ChangeApply { id }).await,
            ChangeAction::Close { id } => cmd_ok(Request::ChangeClose { id }).await,
        },
        Commands::Problem { action } => match action {
            ProblemAction::Open { title, desc } => cmd_ok(Request::ProblemOpen { title, description: desc }).await,
            ProblemAction::List { status } => cmd_problems(Request::ProblemList { status }).await,
            ProblemAction::Show { id } => cmd_problems(Request::ProblemShow { id }).await,
            ProblemAction::Link { problem_id, incident_id } => cmd_ok(Request::ProblemLink { problem_id, incident_id }).await,
            ProblemAction::KnownError { id, root_cause } => cmd_ok(Request::ProblemKnownError { id, root_cause }).await,
            ProblemAction::HypothesisAdd { problem_id, text } => cmd_ok(Request::ProblemHypothesisAdd { problem_id, text }).await,
            ProblemAction::HypothesisList { problem_id } => cmd_hypotheses(Request::ProblemHypothesisList { problem_id }).await,
            ProblemAction::HypothesisStatus { id, status } => cmd_ok(Request::ProblemHypothesisStatus { id, status }).await,
            ProblemAction::Escalate { id, kind, to } => cmd_problem_escalate(&id, &kind, to).await,
            ProblemAction::Close { id } => cmd_ok(Request::ProblemClose { id }).await,
        },
        Commands::Desired { action } => match action {
            DesiredAction::List => cmd_desired_list().await,
            DesiredAction::Show { domain } => cmd_desired_show(&domain).await,
            DesiredAction::Check => cmd_ok(Request::DesiredCheck).await,
        },
        Commands::Metrics { days } => cmd_metrics(days).await,
        Commands::Filters { action } => match action {
            FiltersAction::List => cmd_filters_list().await,
            FiltersAction::Test { domain, text } => cmd_ok(Request::FiltersTest { domain, text }).await,
        },
        Commands::Mode => cmd_mode().await,
        Commands::Posture => cmd_posture().await,
        Commands::Greeting => cmd_greeting().await,
        Commands::Push { action } => match action {
            PushAction::Test => cmd_ok(Request::PushTest).await,
        },
        Commands::Checks => cmd_checks().await,
        Commands::Audit => cmd_audit().await,
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
        Commands::Persona => cmd_persona(),
        Commands::Meeting { action } => match action {
            MeetingAction::Chair { name, agenda, to } => cmd_meeting_chair(&name, agenda, to).await,
        },
        Commands::Plan { cadence, needs, emit, owner, cao } => cmd_plan(&cadence, needs, emit, owner, cao).await,
        Commands::Foreman { action } => match action {
            ForemanAction::RunOnce { dry_run } => cmd_foreman_run_once(dry_run).await,
        },
        Commands::Skill { action } => match action {
            SkillAction::Install { dir } => cmd_skill_install(&dir),
            SkillAction::Packages => cmd_skill_packages(),
        },
        Commands::Deputy { action } => cmd_deputy(action),
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

async fn cmd_foreman_run_once(dry_run: bool) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED};
    use regin_core::foreman::{handover_body, handover_recipient, plan_handover, CaveTask};
    use regin_core::worker;

    let client = BusClient::from_env()?;
    // dry-run peeks (does not consume); a real run consumes the inbox.
    let messages = client.inbox(!dry_run)?;
    let mut handled = 0;
    for m in &messages {
        let Some(task) = CaveTask::from_message(m) else { continue };
        handled += 1;
        if dry_run {
            println!("would run [{}] worker={} task={:?}", task.ref_id.as_deref().unwrap_or("-"), task.worker, task.task);
            continue;
        }
        let kind = match task.worker_kind() {
            Ok(k) => k,
            Err(e) => {
                eprintln!("skipping cave-task: {e}");
                continue;
            }
        };
        // NEEDS-LIVE-VERIFICATION: spawns the claude/opencode worker in the cave.
        let run = worker::run(kind, &task.task, task.cwd.as_deref());
        let (handover, incident) = plan_handover(&task, &run);
        let to = handover_recipient(&task, m);
        client.send(&to, KIND_STRUCTURED, &handover_body(&handover)?, task.ref_id.as_deref())?;
        println!("handover [{}] outcome={} -> {to}", handover.ref_id.as_deref().unwrap_or("-"), handover.outcome);
        // discipline boundary: a broken in-cave step becomes an ITIL incident.
        if let Some(draft) = incident {
            match rpc(&Request::IncidentOpen { title: draft.title, description: draft.description, severity: draft.severity }).await {
                Ok(_) => println!("  opened incident for the failed worker run"),
                Err(e) => eprintln!("  (could not open incident — daemon down? {e})"),
            }
        }
    }
    println!("regin foreman: handled {handled} cave-task(s)");
    Ok(())
}

async fn cmd_problem_escalate(id: &str, kind: &str, to: Option<String>) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED};
    use regin_core::escalation::{body, build, EscalationKind};

    let ekind = EscalationKind::parse(kind)?;
    // fetch the problem so the escalation carries its title + (root-cause) description
    let problem = match rpc(&Request::ProblemShow { id: id.to_string() }).await? {
        Response::Problems { problems } => problems.into_iter().next()
            .ok_or_else(|| anyhow!("no problem {id}"))?,
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("unexpected: {other:?}")),
    };
    let description = problem.root_cause.clone().unwrap_or_else(|| problem.description.clone());

    let client = BusClient::from_env()?;
    let target = to
        .or_else(|| std::env::var("REGIN_ESCALATION_TO").ok())
        .unwrap_or_else(|| "cio@hq".to_string());
    let esc = build(&problem.id, &problem.title, &description, ekind, client.address());
    client.send(&target, KIND_STRUCTURED, &body(&esc)?, Some(&esc.ref_id))?;

    // record the escalation on the regin side (a memory note keyed for recall)
    let note = format!(
        "Escalated problem {} to dvalin as {} (ref {}) -> {target}",
        esc.problem_id, esc.ticket, esc.ref_id
    );
    let _ = rpc(&Request::MemorySave { category: "escalation".into(), content: note.clone() }).await;
    println!("regin: {note}");
    println!("(dvalin will reply with the ticket id, correlated by {})", esc.ref_id);
    Ok(())
}

fn cmd_deputy(action: DeputyAction) -> Result<()> {
    use regin_core::deputy::DeputyStore;
    let store = DeputyStore::from_env()?;
    match action {
        DeputyAction::Assign { role, primary } => {
            let r = store.assign(&role, &primary)?;
            println!("regin: deputy of {} (primary {}) — {:?}", r.role, r.primary, r.state);
        }
        DeputyAction::Show => match store.load()? {
            Some(r) => {
                println!("role:    {}", r.role);
                println!("primary: {}", r.primary);
                println!("state:   {:?}", r.state);
                println!("brief:   {}", if r.brief.is_empty() { "(none)" } else { &r.brief });
            }
            None => println!("regin: no deputy assignment"),
        },
        DeputyAction::Brief { text } => {
            store.set_brief(&text)?;
            println!("regin: continuity brief updated");
        }
        DeputyAction::Activate { confirmed } => {
            let r = store.activate(confirmed)?;
            println!("regin: deputy ACTIVATED for {} — {:?}", r.role, r.state);
        }
        DeputyAction::Handback => {
            let r = store.handback()?;
            println!("regin: handed back to primary {} — {:?}", r.primary, r.state);
        }
    }
    Ok(())
}

fn cmd_skill_install(dir: &std::path::Path) -> Result<()> {
    use regin_core::skillpkg::Package;
    let dest = regin_core::config::user_skills_dir()?;
    let pkg = Package::load(dir)?;
    let installed = pkg.install(&dest)?;
    println!("regin: installed {} ({} skills) into {}", pkg.manifest.name, installed.len(), dest.display());
    for s in installed {
        println!("  + {s}");
    }
    Ok(())
}

fn cmd_skill_packages() -> Result<()> {
    let dest = regin_core::config::user_skills_dir()?;
    let pkgs = regin_core::skillpkg::installed_packages(&dest);
    if pkgs.is_empty() {
        println!("regin: no skill packages installed");
    } else {
        for p in pkgs {
            println!("{p}");
        }
    }
    Ok(())
}

async fn cmd_meeting_chair(name: &str, agenda: Vec<String>, to: Option<String>) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED};
    use regin_core::chair::{collect_reports, compile, minutes_message_body};

    let client = BusClient::from_env()?;
    let reports = collect_reports(&client.inbox(true)?);
    // pull the chair's own open ITIL count (best-effort — discipline feed)
    let open_itil = match rpc(&Request::IncidentList { status: Some("open".into()) }).await {
        Ok(Response::Incidents { incidents }) => incidents.len(),
        _ => 0,
    };
    let agenda = if agenda.is_empty() {
        vec!["incidents & problems".to_string(), "delivery status".to_string(), "priorities".to_string()]
    } else {
        agenda
    };
    let minutes = compile(&agenda, &reports, open_itil);
    let target = to
        .or_else(|| std::env::var("REGIN_MEETING_RECORDER").ok())
        .unwrap_or_else(|| "dvalin@hq".to_string());
    client.send(&target, KIND_STRUCTURED, &minutes_message_body(name, &minutes), Some(name))?;
    println!("regin: chaired {name} — {} report(s), {} decision(s), {} action(s) → {target}",
        reports.len(), minutes.decisions.len(), minutes.action_items.len());
    Ok(())
}

async fn cmd_plan(cadence: &str, needs: Vec<String>, emit: bool, owner: Option<String>, cao: Option<String>) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED};
    use regin_core::planning::{build_plan, capability_gap_body, cadence_scope, priority_ask_body};

    if cadence_scope(cadence).is_none() {
        return Err(anyhow!("unknown cadence {cadence:?} (use weekly|monthly|yearly)"));
    }
    // gather decentralized signals (best-effort — work without the daemon too)
    let schedules: Vec<String> = match rpc(&Request::TaskSchedules).await {
        Ok(Response::SchedulesList { schedules }) => schedules.into_iter().map(|s| s.skill).collect(),
        _ => Vec::new(),
    };
    let count = |resp: Result<Response>| -> usize {
        match resp {
            Ok(Response::Incidents { incidents }) => incidents.len(),
            Ok(Response::Problems { problems }) => problems.len(),
            _ => 0,
        }
    };
    let open_incidents = count(rpc(&Request::IncidentList { status: Some("open".into()) }).await);
    let open_problems = count(rpc(&Request::ProblemList { status: Some("open".into()) }).await);
    let available = regin_core::config::user_skills_dir().ok()
        .map(|d| regin_core::skillpkg::installed_packages(&d)).unwrap_or_default();

    let plan = build_plan(cadence, &schedules, &needs, &available, open_incidents, open_problems);
    println!("{}", serde_json::to_string_pretty(&plan).unwrap_or_default());

    if emit {
        let client = BusClient::from_env()?;
        let owner = owner.or_else(|| std::env::var("REGIN_PROCESS_OWNER").ok());
        if let Some(o) = owner {
            client.send(&o, KIND_STRUCTURED, &priority_ask_body(&plan), Some(cadence))?;
            println!("regin: priority ask → {o}");
        }
        if let Some(body) = capability_gap_body(&plan) {
            let cao = cao.or_else(|| std::env::var("REGIN_CAO").ok()).unwrap_or_else(|| "cao@hq".to_string());
            client.send(&cao, KIND_STRUCTURED, &body, Some(cadence))?;
            println!("regin: capability gap → {cao} ({} gap(s))", plan.capability_gaps.len());
        }
    }
    Ok(())
}

fn cmd_persona() -> Result<()> {
    use regin_core::persona::{Persona, ALL_TOOLS};
    match Persona::from_env()? {
        Some(p) => {
            println!("role:  {}", p.role);
            if !p.title.is_empty() {
                println!("title: {}", p.title);
            }
            let ceiling = if p.tools.is_empty() {
                format!("(unscoped — all tools: {})", ALL_TOOLS.join(", "))
            } else {
                p.tools.join(", ")
            };
            println!("tools: {ceiling}");
            if !p.prompt.is_empty() {
                println!("\n{}", p.prompt);
            }
        }
        None => println!("regin: no persona configured (REGIN_PERSONA unset) — running unscoped"),
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
    // FEAT-043: open with the login greeting (health + parked actionable items).
    if let Ok(Response::GreetingResp { greeting }) = rpc(&Request::GreetingQuery).await {
        render_greeting(&greeting);
    }
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
                if let Some(w) = &i.workaround {
                    println_color(&format!("            workaround: {w}"), Color::DarkGrey);
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
                if let Some(p) = &c.problem_id {
                    println_color(&format!("            problem: {}", sid(p)), Color::DarkGrey);
                }
                if c.before.is_some() || c.after.is_some() {
                    println!(
                        "            {} -> {}",
                        c.before.as_deref().unwrap_or("?"),
                        c.after.as_deref().unwrap_or("?")
                    );
                }
                if let Some(by) = &c.approved_by {
                    println_color(&format!("            approved by {by}"), Color::Green);
                }
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_hypotheses(req: Request) -> Result<()> {
    match rpc(&req).await? {
        Response::Hypotheses { hypotheses } => {
            if hypotheses.is_empty() {
                println!("No hypotheses.");
                return Ok(());
            }
            for h in &hypotheses {
                print_color(&format!("  {}", sid(&h.id)), Color::DarkGrey);
                print_color(&format!("  [{:<10}] ", h.status), Color::Cyan);
                println!("{}", h.text);
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_desired_list() -> Result<()> {
    match rpc(&Request::DesiredList).await? {
        Response::DesiredListResp { items } => {
            if items.is_empty() {
                println!("No desired-state domains. Add files under ~/.config/regin/desired/<domain>.md");
                return Ok(());
            }
            for d in &items {
                print_color(&format!("  {:<16}", d.domain), Color::Cyan);
                print_color(&format!("[{}] ", d.source), Color::DarkGrey);
                print!("{} assertion(s)", d.assertions);
                if let Some(rt) = d.recurrence_threshold {
                    print!(", recurrence>={rt}");
                }
                println!();
                for c in &d.conflicts {
                    println_color(&format!("        conflict: {c}"), Color::Red);
                }
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_desired_show(domain: &str) -> Result<()> {
    match rpc(&Request::DesiredShow { domain: domain.to_string() }).await? {
        Response::DesiredDetail { state } => {
            print_color(&format!("{} ", state.domain), Color::Cyan);
            println_color(&format!("[{}] {}", state.source, state.path.display()), Color::DarkGrey);
            if let Some(rt) = state.recurrence_threshold {
                println_color(&format!("recurrence threshold: {rt}"), Color::DarkGrey);
            }
            if !state.intent.is_empty() {
                println!("\n{}", state.intent);
            }
            if !state.assertions.is_empty() {
                println_color("\nassertions:", Color::Yellow);
                for a in &state.assertions {
                    print!("  {a}");
                    if let Some(d) = &a.description {
                        print_color(&format!("  — {d}"), Color::DarkGrey);
                    }
                    println!();
                }
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

fn fmt_secs(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    }
}

async fn cmd_mode() -> Result<()> {
    match rpc(&Request::ModeQuery).await? {
        Response::ModeInfo { mode, configured, last_ok, failures } => {
            let color = if mode == "org" { Color::Green } else { Color::Yellow };
            print!("effective mode: ");
            println_color(&mode, color);
            println!("  bus configured: {configured}");
            println!("  last reachable: {}", last_ok.as_deref().unwrap_or("never"));
            println!("  consecutive failures: {failures}");
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

fn render_greeting(g: &regin_core::greeting::Greeting) {
    println_color(&g.health_line(), Color::DarkGrey);
    if !g.has_actions() {
        return;
    }
    if !g.pending_changes.is_empty() {
        println_color("changes awaiting approval:", Color::Yellow);
        for a in &g.pending_changes {
            println!("  {}  {}", sid(&a.id), a.title);
        }
    }
    if !g.decision_problems.is_empty() {
        println_color("problems needing a decision:", Color::Yellow);
        for a in &g.decision_problems {
            println!("  {}  {}", sid(&a.id), a.title);
        }
    }
}

/// Generate man pages from the clap surface (FEAT-019) so they never drift from
/// the actual commands. Writes `regin.1` plus a page per visible subcommand.
fn cmd_gen_man(dir: &str) -> Result<()> {
    let out = std::path::Path::new(dir);
    std::fs::create_dir_all(out).with_context(|| format!("create {dir}"))?;
    let cmd = Cli::command();

    let mut buf = Vec::new();
    clap_mangen::Man::new(cmd.clone()).render(&mut buf)?;
    std::fs::write(out.join("regin.1"), &buf).context("write regin.1")?;

    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        let name = sub.get_name();
        let mut b = Vec::new();
        clap_mangen::Man::new(sub.clone()).render(&mut b)?;
        std::fs::write(out.join(format!("regin-{name}.1")), &b)
            .with_context(|| format!("write regin-{name}.1"))?;
    }
    println!("man pages written to {dir}");
    Ok(())
}

async fn cmd_audit() -> Result<()> {
    match rpc(&Request::AuditRun).await? {
        Response::AuditResult { findings, trimmed, opened } => {
            if trimmed {
                println_color("(audit trimmed to stay within budget)", Color::DarkGrey);
            }
            if findings.is_empty() {
                println_color("Self-audit clean — no findings.", Color::Green);
                return Ok(());
            }
            for f in &findings {
                print_color(&format!("  [{}] ", f.area), Color::Yellow);
                println!("{}", f.message);
            }
            println_color(&format!("{opened} new problem(s) filed for review.", ), Color::DarkGrey);
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_checks() -> Result<()> {
    match rpc(&Request::ChecksList).await? {
        Response::DerivedChecks { checks } => {
            if checks.is_empty() {
                println!("No derived checks yet. regin promotes stable LLM verdicts into cheap checks over time.");
                return Ok(());
            }
            for c in &checks {
                print_color(&format!("  {:<16}", c.domain), Color::Cyan);
                print!("{}", c.description);
                println_color(&format!("  [{}]", c.signature), Color::DarkGrey);
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_greeting() -> Result<()> {
    match rpc(&Request::GreetingQuery).await? {
        Response::GreetingResp { greeting } => render_greeting(&greeting),
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_posture() -> Result<()> {
    match rpc(&Request::PostureQuery).await? {
        Response::PostureInfo { posture, allow_auto, change_successes, change_failures, change_success_rate, promotion_error_rate } => {
            let color = if posture == "trusted" { Color::Green } else { Color::Yellow };
            print!("autonomy posture: ");
            println_color(&posture, color);
            println!("  master switch (posture.allow_auto): {allow_auto}");
            println!("  change outcomes: {change_successes} ok / {change_failures} failed ({:.0}% success)", change_success_rate * 100.0);
            println!("  promotion error rate: {:.0}%", promotion_error_rate * 100.0);
            if posture == "conservative" {
                println_color("  safe fixes still route to approval until trust is earned", Color::DarkGrey);
            } else {
                println_color("  safe, reversible fixes may auto-apply", Color::DarkGrey);
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_filters_list() -> Result<()> {
    match rpc(&Request::FiltersList).await? {
        Response::Filters { rules } => {
            if rules.is_empty() {
                println!("No notice filters. Add rule files under ~/.config/regin/filters/*.toml");
                return Ok(());
            }
            for r in &rules {
                print_color(&format!("  {:<20}", r.name), Color::Cyan);
                print_color(&format!("[{}] ", r.source), Color::DarkGrey);
                print!("contains {:?}", r.contains);
                if let Some(d) = &r.domain {
                    print!(" (domain: {d})");
                }
                println!();
            }
        }
        Response::Error { message } => return Err(anyhow!("{message}")),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_metrics(days: Option<u32>) -> Result<()> {
    match rpc(&Request::Metrics { since_days: days }).await? {
        Response::Metrics { summary: s, objective: o } => {
            println_color(&format!("CSI metrics — last {} days", days.unwrap_or(30)), Color::Cyan);

            println_color("\nObjective (minimize cost s.t. reliability >= floor)", Color::Yellow);
            let verdict = if o.meets_floor { "MEETS floor" } else { "BELOW floor" };
            let vcolor = if o.meets_floor { Color::Green } else { Color::Red };
            print!("  reliability {:.0}% (floor {:.0}%)  ", o.reliability * 100.0, o.reliability_floor * 100.0);
            println_color(verdict, vcolor);
            println!("  LLM cost: ${:.2}", o.cost_llm_usd);

            println_color("\nReliability / quality", Color::Yellow);
            println!("  incidents: {} opened, {} resolved, {} open", s.incidents_opened, s.incidents_resolved, s.open_incidents);
            println!("  time in deviation: {}", fmt_secs(s.time_in_deviation_secs));
            match s.mttr_secs {
                Some(m) => println!("  MTTR: {}", fmt_secs(m)),
                None => println!("  MTTR: n/a"),
            }
            println!("  recurring problems: {}", s.recurring_problems);

            println_color("\nAutomation / autonomy", Color::Yellow);
            println!("  remediations: {} auto, {} approved, {} escalated", s.remediations_auto, s.remediations_approved, s.remediations_escalated);
            println!("  automation ratio: {:.0}%   autonomy ratio: {:.0}%", s.automation_ratio * 100.0, s.autonomy_ratio * 100.0);

            println_color("\nCost / efficiency", Color::Yellow);
            println!("  LLM spend: ${:.2}   avoided: ${:.2}   notices filtered: {}", s.cost_llm_usd, s.cost_avoided_usd, s.notice_filter_saved);

            println_color("\nLearning / health", Color::Yellow);
            println!("  promotions: {}   errors: {}   error rate: {:.0}%", s.promotions, s.promotion_errors, s.promotion_error_rate * 100.0);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_command_tree_is_valid() {
        // clap's own structural validation (dup args, bad option specs, etc.).
        Cli::command().debug_assert();
    }

    #[test]
    fn man_pages_generate_from_clap() {
        let dir = std::env::temp_dir().join(format!("regin-man-test-{}", std::process::id()));
        cmd_gen_man(dir.to_str().unwrap()).unwrap();
        assert!(dir.join("regin.1").exists(), "top-level man page");
        // a page per visible subcommand
        assert!(dir.join("regin-audit.1").exists());
        assert!(dir.join("regin-metrics.1").exists());
        // the hidden gen-man subcommand is not documented
        assert!(!dir.join("regin-gen-man.1").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
