mod render;
mod transport;

use std::io::{self, BufRead, Write};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use regin_core::{
    config,
    protocol::{Request, Response},
    types::ChatMessage,
};
use render::*;
use transport::{install_regind_service, SocketTransport, Transport};

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
///   regin config set mimir.fingerprint <credential>
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
        \x20 regin config set mimir.fingerprint YOUR_CREDENTIAL\n\
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
    ///   mimir.fingerprint   Mimir access credential — approved client-cert fingerprint (required)
    ///   mimir.model         LLM model (default: auto — Mimir routes it)
    ///   mimir.base_url      Mimir gateway OpenAI-compatible base URL
    ///   daemon.enabled      Run regind as persistent user service (true/false)
    #[command(after_help = "EXAMPLES:\n\
        \x20 regin config set mimir.fingerprint a1b2c3...\n\
        \x20 regin config set mimir.model gpt-4o\n\
        \x20 regin config set daemon.enabled true\n\
        \x20 regin config get mimir.model\n\
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
    /// draft it (requires mimir.fingerprint). Refuses to overwrite an existing
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
        /// Setting key (e.g. mimir.fingerprint)
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

    /// Export the identity database to a portable snapshot file.
    Export {
        /// Output path for the snapshot
        path: String,
    },

    /// Import a portable identity snapshot.
    Import {
        /// Path to the snapshot file
        path: String,
        /// Merge without overwriting existing memories (default: refuse if identity exists)
        #[arg(long)]
        merge: bool,
    },

    /// Show identity metadata.
    Info,
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
    let t = SocketTransport;

    match cli.command {
        Commands::GenMan { dir } => cmd_gen_man(&dir),
        Commands::Chat => cmd_chat(&t).await,
        Commands::Task { action } => match action {
            TaskAction::List => cmd_task_list(&t).await,
            TaskAction::Show { name } => cmd_task_show(&t, &name).await,
            TaskAction::Exec { name, schedule } => cmd_task_exec(&t, &name, schedule.as_deref()).await,
            TaskAction::Unschedule { name } => cmd_task_unschedule(&t, &name).await,
            TaskAction::Schedules => cmd_task_schedules(&t).await,
            TaskAction::Create { name, from_prompt, force, edit, repo } => {
                cmd_task_create(&t, &name, from_prompt, force, edit, repo).await
            }
        },
        Commands::Runs { skill, limit } => cmd_runs(&t, skill.as_deref(), limit).await,
        Commands::Config { action } => match action {
            ConfigAction::List => cmd_config_list(&t).await,
            ConfigAction::Get { key } => cmd_config_get(&t, &key).await,
            ConfigAction::Set { key, value } => cmd_config_set(&t, &key, &value).await,
        },
        Commands::Memory { action } => match action {
            MemoryAction::List { category } => cmd_memory_list(&t, category.as_deref()).await,
            MemoryAction::Search { query } => cmd_memory_search(&t, &query).await,
            MemoryAction::Save { category, content } => cmd_memory_save(&t, &category, &content).await,
            MemoryAction::Update { id, content } => cmd_memory_update(&t, &id, &content).await,
            MemoryAction::Delete { id } => cmd_memory_delete(&t, &id).await,
            MemoryAction::Reflect => cmd_memory_reflect(&t).await,
            MemoryAction::Export { path } => cmd_memory_export(&t, &path).await,
            MemoryAction::Import { path, merge } => cmd_memory_import(&t, &path, merge).await,
            MemoryAction::Info => cmd_memory_info(&t).await,
        },
        Commands::Incident { action } => match action {
            IncidentAction::Open { title, severity, desc } => {
                cmd_ok(&t, Request::IncidentOpen { title, description: desc, severity }).await
            }
            IncidentAction::List { status } => cmd_incidents(&t, Request::IncidentList { status }).await,
            IncidentAction::Show { id } => cmd_incidents(&t, Request::IncidentShow { id }).await,
            IncidentAction::Update { id, status } => cmd_ok(&t, Request::IncidentUpdate { id, status }).await,
            IncidentAction::Resolve { id, resolution } => cmd_ok(&t, Request::IncidentResolve { id, resolution }).await,
            IncidentAction::Block { id, workaround } => cmd_ok(&t, Request::IncidentBlock { id, workaround }).await,
            IncidentAction::Close { id } => cmd_ok(&t, Request::IncidentClose { id }).await,
        },
        Commands::Change { action } => match action {
            ChangeAction::Record { title, desc, incident, problem, before, after } => {
                cmd_ok(&t, Request::ChangeRecord { title, description: desc, incident_id: incident, problem_id: problem, before, after }).await
            }
            ChangeAction::List => cmd_changes(&t, Request::ChangeList).await,
            ChangeAction::Show { id } => cmd_changes(&t, Request::ChangeShow { id }).await,
            ChangeAction::RequestApproval { id } => cmd_ok(&t, Request::ChangeRequestApproval { id }).await,
            ChangeAction::Approve { id, by } => cmd_ok(&t, Request::ChangeApprove { id, approved_by: by }).await,
            ChangeAction::Apply { id } => cmd_ok(&t, Request::ChangeApply { id }).await,
            ChangeAction::Close { id } => cmd_ok(&t, Request::ChangeClose { id }).await,
        },
        Commands::Problem { action } => match action {
            ProblemAction::Open { title, desc } => cmd_ok(&t, Request::ProblemOpen { title, description: desc }).await,
            ProblemAction::List { status } => cmd_problems(&t, Request::ProblemList { status }).await,
            ProblemAction::Show { id } => cmd_problems(&t, Request::ProblemShow { id }).await,
            ProblemAction::Link { problem_id, incident_id } => cmd_ok(&t, Request::ProblemLink { problem_id, incident_id }).await,
            ProblemAction::KnownError { id, root_cause } => cmd_ok(&t, Request::ProblemKnownError { id, root_cause }).await,
            ProblemAction::HypothesisAdd { problem_id, text } => cmd_ok(&t, Request::ProblemHypothesisAdd { problem_id, text }).await,
            ProblemAction::HypothesisList { problem_id } => cmd_hypotheses(&t, Request::ProblemHypothesisList { problem_id }).await,
            ProblemAction::HypothesisStatus { id, status } => cmd_ok(&t, Request::ProblemHypothesisStatus { id, status }).await,
            ProblemAction::Escalate { id, kind, to } => cmd_problem_escalate(&t, &id, &kind, to).await,
            ProblemAction::Close { id } => cmd_ok(&t, Request::ProblemClose { id }).await,
        },
        Commands::Desired { action } => match action {
            DesiredAction::List => cmd_desired_list(&t).await,
            DesiredAction::Show { domain } => cmd_desired_show(&t, &domain).await,
            DesiredAction::Check => cmd_ok(&t, Request::DesiredCheck).await,
        },
        Commands::Metrics { days } => cmd_metrics(&t, days).await,
        Commands::Filters { action } => match action {
            FiltersAction::List => cmd_filters_list(&t).await,
            FiltersAction::Test { domain, text } => cmd_ok(&t, Request::FiltersTest { domain, text }).await,
        },
        Commands::Mode => cmd_mode(&t).await,
        Commands::Posture => cmd_posture(&t).await,
        Commands::Greeting => cmd_greeting(&t).await,
        Commands::Push { action } => match action {
            PushAction::Test => cmd_ok(&t, Request::PushTest).await,
        },
        Commands::Checks => cmd_checks(&t).await,
        Commands::Audit => cmd_audit(&t).await,
        Commands::Context { action } => match action {
            ContextAction::Show => cmd_context_show(&t).await,
            ContextAction::Set { content } => {
                cmd_ok(&t, Request::ContextSet { cwd: Some(cwd_string()), content }).await
            }
        },
        Commands::Bus { action } => match action {
            BusAction::Send { to, body, structured, ref_id } => cmd_bus_send(&to, &body, structured, ref_id.as_deref()),
            BusAction::Inbox { peek } => cmd_bus_inbox(peek),
        },
        Commands::Persona => cmd_persona(),
        Commands::Meeting { action } => match action {
            MeetingAction::Chair { name, agenda, to } => cmd_meeting_chair(&t, &name, agenda, to).await,
        },
        Commands::Plan { cadence, needs, emit, owner, cao } => cmd_plan(&t, &cadence, needs, emit, owner, cao).await,
        Commands::Foreman { action } => match action {
            ForemanAction::RunOnce { dry_run } => cmd_foreman_run_once(&t, dry_run).await,
        },
        Commands::Skill { action } => match action {
            SkillAction::Install { dir } => cmd_skill_install(&dir),
            SkillAction::Packages => cmd_skill_packages(),
        },
        Commands::Deputy { action } => cmd_deputy(action),
        Commands::Ping => cmd_ping(&t).await,
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

async fn cmd_foreman_run_once(t: &impl Transport, dry_run: bool) -> Result<()> {
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
            match t.request(&Request::IncidentOpen { title: draft.title, description: draft.description, severity: draft.severity }).await {
                Ok(_) => println!("  opened incident for the failed worker run"),
                Err(e) => eprintln!("  (could not open incident — daemon down? {e})"),
            }
        }
    }
    println!("regin foreman: handled {handled} cave-task(s)");
    Ok(())
}

async fn cmd_problem_escalate(t: &impl Transport, id: &str, kind: &str, to: Option<String>) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED};
    use regin_core::escalation::{body, build, EscalationKind};

    let ekind = EscalationKind::parse(kind)?;
    // fetch the problem so the escalation carries its title + (root-cause) description
    let problem = match t.request(&Request::ProblemShow { id: id.to_string() }).await? {
        Response::Problems { problems } => problems.into_iter().next()
            .ok_or_else(|| anyhow!("no problem {id}"))?,
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
    let _ = t.request(&Request::MemorySave { category: "escalation".into(), content: note.clone() }).await;
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

async fn cmd_meeting_chair(t: &impl Transport, name: &str, agenda: Vec<String>, to: Option<String>) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED};
    use regin_core::chair::{collect_reports, compile, minutes_message_body};

    let client = BusClient::from_env()?;
    let reports = collect_reports(&client.inbox(true)?);
    // pull the chair's own open ITIL count (best-effort — discipline feed)
    let open_itil = match t.request(&Request::IncidentList { status: Some("open".into()) }).await {
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

async fn cmd_plan(t: &impl Transport, cadence: &str, needs: Vec<String>, emit: bool, owner: Option<String>, cao: Option<String>) -> Result<()> {
    use regin_core::bus::{BusClient, KIND_STRUCTURED};
    use regin_core::planning::{build_plan, capability_gap_body, cadence_scope, priority_ask_body};

    if cadence_scope(cadence).is_none() {
        return Err(anyhow!("unknown cadence {cadence:?} (use weekly|monthly|yearly)"));
    }
    // gather decentralized signals (best-effort — work without the daemon too)
    let schedules: Vec<String> = match t.request(&Request::TaskSchedules).await {
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
    let open_incidents = count(t.request(&Request::IncidentList { status: Some("open".into()) }).await);
    let open_problems = count(t.request(&Request::ProblemList { status: Some("open".into()) }).await);
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

// Daemon auto-start + socket plumbing lives in `transport.rs` (FEAT-070).

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

/// Print one chat-stream event live (colors included) and fold it into
/// (full, conv_id) via the pure `apply_chat_event`.
fn print_chat_event(resp: &Response, full: &mut String, conv_id: &mut String) {
    match resp {
        Response::StreamChunk { token } => {
            print_color(token, Color::Green);
            let _ = io::stdout().flush();
        }
        Response::ToolCallEvent { name, arguments } => {
            println!();
            println_color(&render_tool_call(name, arguments), Color::Magenta);
        }
        Response::ToolResultEvent { name, success, output } => {
            let color = if *success { Color::Green } else { Color::Red };
            println_color(&render_tool_result(name, *success, output, 5), color);
        }
        Response::Error { message } => {
            println_color(&format!("\n[error: {message}]"), Color::Red);
        }
        _ => {}
    }
    apply_chat_event(resp, full, conv_id);
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn cmd_chat(t: &impl Transport) -> Result<()> {
    let mut conv_id = match t.request(&Request::ChatNew).await? {
        Response::ChatNew { conversation_id } => conversation_id,
        resp => return Err(anyhow!("Unexpected: {resp:?}")),
    };

    println_color("regin — Linux server administration agent", Color::Yellow);
    println_color("Commands: /new  /history  /quit", Color::DarkGrey);
    // FEAT-043: open with the login greeting (health + parked actionable items).
    if let Ok(Response::GreetingResp { greeting }) = t.request(&Request::GreetingQuery).await {
        print!("{}", render_greeting(&greeting));
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
                conv_id = match t.request(&Request::ChatNew).await? {
                    Response::ChatNew { conversation_id } => conversation_id,
                    _ => continue,
                };
                history.clear();
                println_color("— new conversation —", Color::Yellow);
                println!();
                continue;
            }
            "/history" => {
                if let Response::ChatHistory { conversations } = t.request(&Request::ChatHistory).await? {
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

        print_color("regin> ", Color::Green);
        io::stdout().flush()?;

        let mut full = String::new();
        let mut new_conv = String::new();
        t.request_stream(
            &Request::ChatSend { conversation_id: conv_id.clone(), messages: history.clone(), cwd: Some(cwd_string()) },
            |resp| print_chat_event(resp, &mut full, &mut new_conv),
        ).await?;
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

async fn cmd_task_list(t: &impl Transport) -> Result<()> {
    match t.request(&Request::SkillList { cwd: Some(cwd_string()) }).await? {
        Response::SkillList { skills } => print!("{}", render_task_list(&skills)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_show(t: &impl Transport, name: &str) -> Result<()> {
    match t.request(&Request::SkillShow { name: name.into(), cwd: Some(cwd_string()) }).await? {
        Response::SkillDetail { name, description, prompt, files } => {
            print!("{}", render_task_show(&name, &description, &prompt, &files));
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_exec(t: &impl Transport, name: &str, schedule: Option<&str>) -> Result<()> {
    if let Some(interval) = schedule {
        match t.request(&Request::TaskSchedule { skill: name.into(), interval: interval.into() }).await {
            Ok(Response::Ok { message }) => println_color(&format!("✓ {message}"), Color::Yellow),
            Ok(_) => {}
            Err(e) => return Err(anyhow!("Schedule error: {e}")),
        }
    }

    println_color(&format!("Running '{name}'…"), Color::Yellow);
    let events = t.request_stream(
        &Request::TaskExec { skill: name.into(), cwd: Some(cwd_string()) },
        |resp| match resp {
            Response::ToolCallEvent { name, arguments } => {
                println_color(&render_tool_call(name, arguments), Color::Magenta);
            }
            Response::ToolResultEvent { name, success, output } => {
                let color = if *success { Color::Green } else { Color::Red };
                println_color(&render_tool_result(name, *success, output, 3), color);
            }
            Response::StreamChunk { token } => {
                print_color(token, Color::Green);
                let _ = io::stdout().flush();
            }
            _ => {}
        },
    ).await?;

    match events.last() {
        Some(Response::TaskResult { run }) => {
            let color = if run.status == "success" { Color::Green } else { Color::Red };
            println_color(&render_task_result(run), color);
        }
        Some(Response::Error { message }) => println_color(&format!("Error: {message}"), Color::Red),
        _ => {}
    }
    Ok(())
}

async fn cmd_task_unschedule(t: &impl Transport, name: &str) -> Result<()> {
    match t.request(&Request::TaskUnschedule { skill: name.into() }).await? {
        Response::Ok { message } => print!("{}", render_ok(&message)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_schedules(t: &impl Transport) -> Result<()> {
    match t.request(&Request::TaskSchedules).await? {
        Response::SchedulesList { schedules } => print!("{}", render_schedules(&schedules)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_runs(t: &impl Transport, skill: Option<&str>, limit: u32) -> Result<()> {
    match t.request(&Request::RunsList { skill: skill.map(String::from), limit }).await? {
        Response::RunsList { runs } => print!("{}", render_runs(&runs)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_config_list(t: &impl Transport) -> Result<()> {
    match t.request(&Request::ConfigList).await? {
        Response::ConfigEntries { entries } => print!("{}", render_config_entries(&entries)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_config_get(t: &impl Transport, key: &str) -> Result<()> {
    match t.request(&Request::ConfigGet { key: key.into() }).await? {
        Response::ConfigValue { key, value } => print!("{}", render_config_value(&key, &value)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_config_set(t: &impl Transport, key: &str, value: &str) -> Result<()> {
    if key == "daemon.enabled" {
        handle_daemon_enabled(value).await?;
    }
    match t.request(&Request::ConfigSet { key: key.into(), value: value.into() }).await? {
        Response::Ok { message } => print!("{}", render_ok(&message)),
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

async fn cmd_memory_list(t: &impl Transport, category: Option<&str>) -> Result<()> {
    match t.request(&Request::MemoryList { category: category.map(String::from) }).await? {
        Response::MemoryList { memories } => print!("{}", render_memory_list(&memories)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_search(t: &impl Transport, query: &str) -> Result<()> {
    match t.request(&Request::MemorySearch { query: query.into() }).await? {
        Response::MemoryList { memories } => print!("{}", render_memory_search(&memories)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_save(t: &impl Transport, category: &str, content: &str) -> Result<()> {
    match t.request(&Request::MemorySave { category: category.into(), content: content.into() }).await? {
        Response::Ok { message } => print!("{}", render_ok(&message)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_update(t: &impl Transport, id: &str, content: &str) -> Result<()> {
    match t.request(&Request::MemoryUpdate { id: id.into(), content: content.into() }).await? {
        Response::Ok { message } => print!("{}", render_ok(&message)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_delete(t: &impl Transport, id: &str) -> Result<()> {
    match t.request(&Request::MemoryDelete { id: id.into() }).await? {
        Response::Ok { message } => print!("{}", render_ok(&message)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_export(t: &impl Transport, path: &str) -> Result<()> {
    match t.request(&Request::MemoryExport { path: path.into() }).await? {
        Response::MemoryExport { path: p } => print!("{}", render_memory_export(&p)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_import(t: &impl Transport, path: &str, merge: bool) -> Result<()> {
    match t.request(&Request::MemoryImport { path: path.into(), merge }).await? {
        Response::Ok { message } => print!("{}", render_ok(&message)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_info(t: &impl Transport) -> Result<()> {
    match t.request(&Request::MemoryInfo).await? {
        Response::MemoryInfo { identity_id, name, host, schema_version, memory_count, created_at } => {
            print!("{}", render_memory_info(&identity_id, &name, &host, &schema_version, memory_count, &created_at));
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_task_create(
    t: &impl Transport,
    name: &str,
    from_prompt: Option<String>,
    force: bool,
    edit: bool,
    repo: bool,
) -> Result<()> {
    let cwd = repo.then(cwd_string);
    match t.request(&Request::TaskCreate { name: name.into(), from_prompt, force, repo, cwd }).await? {
        Response::SkillCreated { path, shadows_system } => {
            print!("{}", render_task_created(name, &path, shadows_system));
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
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_context_show(t: &impl Transport) -> Result<()> {
    match t.request(&Request::ContextShow { cwd: Some(cwd_string()) }).await? {
        Response::Context { repo_key, content } => {
            print!("{}", render_context_show(repo_key.as_deref(), content.as_deref()));
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_memory_reflect(t: &impl Transport) -> Result<()> {
    match t.request(&Request::MemoryReflect).await? {
        Response::ReflectStats { episodes, reinforced, created, decayed } => {
            print!("{}", render_reflect_stats(episodes, reinforced, created, decayed));
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_ping(t: &impl Transport) -> Result<()> {
    match t.request(&Request::Ping).await? {
        Response::Pong => print!("{}", render_ping(true)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

// --- ITIL command helpers ---

/// Run a request that returns a simple Ok/Error acknowledgement.
async fn cmd_ok(t: &impl Transport, req: Request) -> Result<()> {
    match t.request(&req).await? {
        Response::Ok { message } => print!("{}", render_ok(&message)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_incidents(t: &impl Transport, req: Request) -> Result<()> {
    match t.request(&req).await? {
        Response::Incidents { incidents } => print!("{}", render_incidents(&incidents)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_changes(t: &impl Transport, req: Request) -> Result<()> {
    match t.request(&req).await? {
        Response::Changes { changes } => print!("{}", render_changes(&changes)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_hypotheses(t: &impl Transport, req: Request) -> Result<()> {
    match t.request(&req).await? {
        Response::Hypotheses { hypotheses } => print!("{}", render_hypotheses(&hypotheses)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_desired_list(t: &impl Transport) -> Result<()> {
    match t.request(&Request::DesiredList).await? {
        Response::DesiredListResp { items } => print!("{}", render_desired_list(&items)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_desired_show(t: &impl Transport, domain: &str) -> Result<()> {
    match t.request(&Request::DesiredShow { domain: domain.to_string() }).await? {
        Response::DesiredDetail { state } => print!("{}", render_desired_show(&state)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_mode(t: &impl Transport) -> Result<()> {
    match t.request(&Request::ModeQuery).await? {
        Response::ModeInfo { mode, configured, last_ok, failures } => {
            print!("{}", render_mode(&mode, configured, last_ok.as_deref(), failures));
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
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

async fn cmd_audit(t: &impl Transport) -> Result<()> {
    match t.request(&Request::AuditRun).await? {
        Response::AuditResult { findings, trimmed, opened } => print!("{}", render_audit(&findings, trimmed, opened)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_checks(t: &impl Transport) -> Result<()> {
    match t.request(&Request::ChecksList).await? {
        Response::DerivedChecks { checks } => print!("{}", render_checks(&checks)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_greeting(t: &impl Transport) -> Result<()> {
    match t.request(&Request::GreetingQuery).await? {
        Response::GreetingResp { greeting } => print!("{}", render_greeting(&greeting)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_posture(t: &impl Transport) -> Result<()> {
    match t.request(&Request::PostureQuery).await? {
        Response::PostureInfo { posture, allow_auto, change_successes, change_failures, change_success_rate, promotion_error_rate } => {
            print!("{}", render_posture(&posture, allow_auto, change_successes, change_failures, change_success_rate, promotion_error_rate));
        }
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_filters_list(t: &impl Transport) -> Result<()> {
    match t.request(&Request::FiltersList).await? {
        Response::Filters { rules } => print!("{}", render_filters(&rules)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_metrics(t: &impl Transport, days: Option<u32>) -> Result<()> {
    match t.request(&Request::Metrics { since_days: days }).await? {
        Response::Metrics { summary, objective } => print!("{}", render_metrics(&summary, &objective, days)),
        other => return Err(anyhow!("Unexpected: {other:?}")),
    }
    Ok(())
}

async fn cmd_problems(t: &impl Transport, req: Request) -> Result<()> {
    match t.request(&req).await? {
        Response::Problems { problems } => print!("{}", render_problems(&problems)),
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

    // -----------------------------------------------------------------
    // clap surface (FEAT-070 acceptance criterion 2)
    // -----------------------------------------------------------------

    fn parses(argv: &[&str]) {
        Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("failed to parse {argv:?}: {e}"));
    }

    #[test]
    fn clap_parses_representative_commands_across_the_tree() {
        parses(&["regin", "chat"]);
        parses(&["regin", "ping"]);
        parses(&["regin", "task", "list"]);
        parses(&["regin", "task", "show", "disk-usage"]);
        parses(&["regin", "task", "exec", "disk-usage"]);
        parses(&["regin", "task", "exec", "disk-usage", "daily"]);
        parses(&["regin", "task", "unschedule", "disk-usage"]);
        parses(&["regin", "task", "schedules"]);
        parses(&["regin", "task", "create", "my-skill", "--from-prompt", "check disk", "--force", "--edit", "--repo"]);
        parses(&["regin", "runs", "--skill", "disk-usage", "--limit", "5"]);
        parses(&["regin", "config", "list"]);
        parses(&["regin", "config", "get", "mimir.model"]);
        parses(&["regin", "config", "set", "mimir.model", "gpt-4o"]);
        parses(&["regin", "memory", "list", "--category", "fact"]);
        parses(&["regin", "memory", "search", "postgres"]);
        parses(&["regin", "memory", "save", "fact", "runs Ubuntu"]);
        parses(&["regin", "memory", "update", "id1", "new content"]);
        parses(&["regin", "memory", "delete", "id1"]);
        parses(&["regin", "memory", "reflect"]);
        parses(&["regin", "memory", "export", "/tmp/out.db"]);
        parses(&["regin", "memory", "import", "/tmp/out.db", "--merge"]);
        parses(&["regin", "memory", "info"]);
        parses(&["regin", "incident", "open", "disk full", "--severity", "high", "--desc", "d"]);
        parses(&["regin", "incident", "list", "--status", "open"]);
        parses(&["regin", "incident", "show", "id1"]);
        parses(&["regin", "incident", "update", "id1", "--status", "investigating"]);
        parses(&["regin", "incident", "resolve", "id1", "fixed it"]);
        parses(&["regin", "incident", "block", "id1", "restarted service"]);
        parses(&["regin", "incident", "close", "id1"]);
        parses(&["regin", "change", "record", "bump disk", "--incident", "id1"]);
        parses(&["regin", "change", "list"]);
        parses(&["regin", "change", "show", "id1"]);
        parses(&["regin", "change", "request-approval", "id1"]);
        parses(&["regin", "change", "approve", "id1", "--by", "rene"]);
        parses(&["regin", "change", "apply", "id1"]);
        parses(&["regin", "change", "close", "id1"]);
        parses(&["regin", "problem", "open", "recurring disk full"]);
        parses(&["regin", "problem", "list", "--status", "open"]);
        parses(&["regin", "problem", "show", "id1"]);
        parses(&["regin", "problem", "link", "p1", "i1"]);
        parses(&["regin", "problem", "known-error", "p1", "log rotation"]);
        parses(&["regin", "problem", "hypothesis-add", "p1", "cron misfires"]);
        parses(&["regin", "problem", "hypothesis-list", "p1"]);
        parses(&["regin", "problem", "hypothesis-status", "h1", "confirmed"]);
        parses(&["regin", "problem", "escalate", "p1", "--as", "bug", "--to", "cio@hq"]);
        parses(&["regin", "problem", "close", "p1"]);
        parses(&["regin", "desired", "list"]);
        parses(&["regin", "desired", "show", "disk"]);
        parses(&["regin", "desired", "check"]);
        parses(&["regin", "metrics", "--days", "7"]);
        parses(&["regin", "filters", "list"]);
        parses(&["regin", "filters", "test", "network", "connection reset"]);
        parses(&["regin", "mode"]);
        parses(&["regin", "posture"]);
        parses(&["regin", "greeting"]);
        parses(&["regin", "push", "test"]);
        parses(&["regin", "checks"]);
        parses(&["regin", "audit"]);
        parses(&["regin", "context", "show"]);
        parses(&["regin", "context", "set", "notes"]);
        parses(&["regin", "bus", "send", "role@cave", "hello", "--structured", "--ref-id", "r1"]);
        parses(&["regin", "bus", "inbox", "--peek"]);
        parses(&["regin", "persona"]);
        parses(&["regin", "meeting", "chair", "board", "--agenda", "incidents", "--to", "dvalin@hq"]);
        parses(&["regin", "plan", "--cadence", "monthly", "--need", "rust", "--emit"]);
        parses(&["regin", "foreman", "run-once", "--dry-run"]);
        parses(&["regin", "skill", "install", "/path/to/pkg"]);
        parses(&["regin", "skill", "packages"]);
        parses(&["regin", "deputy", "assign", "operator", "regin@host1"]);
        parses(&["regin", "deputy", "show"]);
        parses(&["regin", "deputy", "brief", "text"]);
        parses(&["regin", "deputy", "activate", "--confirmed"]);
        parses(&["regin", "deputy", "handback"]);
    }

    #[test]
    fn clap_rejects_missing_required_args() {
        assert!(Cli::try_parse_from(["regin", "task", "show"]).is_err());
        assert!(Cli::try_parse_from(["regin", "incident", "show"]).is_err());
        assert!(Cli::try_parse_from(["regin", "memory", "save", "fact"]).is_err());
    }

    // -----------------------------------------------------------------
    // cmd_* logic via FakeTransport (FEAT-070 acceptance criterion 1)
    // -----------------------------------------------------------------

    use crate::transport::fake::FakeTransport;
    use regin_core::types::TaskRun;

    #[tokio::test]
    async fn cmd_ping_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::Pong);
        assert!(cmd_ping(&t).await.is_ok());
        assert!(matches!(t.sent()[0], Request::Ping));

        let t = FakeTransport::new();
        t.push(Response::Error { message: "daemon down".into() });
        let err = cmd_ping(&t).await.unwrap_err();
        assert_eq!(err.to_string(), "daemon down");
    }

    #[tokio::test]
    async fn cmd_task_list_sends_skill_list_request() {
        let t = FakeTransport::new();
        t.push(Response::SkillList { skills: vec![] });
        assert!(cmd_task_list(&t).await.is_ok());
        assert!(matches!(t.sent()[0], Request::SkillList { .. }));

        let t = FakeTransport::new();
        t.push(Response::Error { message: "no daemon".into() });
        assert!(cmd_task_list(&t).await.is_err());
    }

    #[tokio::test]
    async fn cmd_task_show_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::SkillDetail { name: "disk-usage".into(), description: "d".into(), prompt: "p".into(), files: vec![] });
        assert!(cmd_task_show(&t, "disk-usage").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "not found".into() });
        assert!(cmd_task_show(&t, "nope").await.is_err());
    }

    #[tokio::test]
    async fn cmd_task_exec_runs_without_schedule() {
        let t = FakeTransport::new();
        t.push_stream(vec![
            Response::ToolCallEvent { name: "bash".into(), arguments: "{}".into() },
            Response::ToolResultEvent { name: "bash".into(), success: true, output: "ok".into() },
            Response::TaskResult { run: TaskRun {
                id: "1".into(), skill_name: "disk-usage".into(), status: "success".into(),
                output: "done".into(), started_at: "t0".into(), finished_at: "t1".into(),
            }},
        ]);
        assert!(cmd_task_exec(&t, "disk-usage", None).await.is_ok());
        assert!(matches!(t.sent()[0], Request::TaskExec { .. }));
    }

    #[tokio::test]
    async fn cmd_task_exec_schedules_first_when_given_an_interval() {
        let t = FakeTransport::new();
        t.push(Response::Ok { message: "scheduled".into() });
        t.push_stream(vec![Response::TaskResult { run: TaskRun {
            id: "1".into(), skill_name: "disk-usage".into(), status: "success".into(),
            output: "done".into(), started_at: "t0".into(), finished_at: "t1".into(),
        }}]);
        assert!(cmd_task_exec(&t, "disk-usage", Some("daily")).await.is_ok());
        assert!(matches!(t.sent()[0], Request::TaskSchedule { .. }));
        assert!(matches!(t.sent()[1], Request::TaskExec { .. }));
    }

    #[tokio::test]
    async fn cmd_task_unschedule_error_path() {
        let t = FakeTransport::new();
        t.push(Response::Error { message: "not scheduled".into() });
        assert!(cmd_task_unschedule(&t, "disk-usage").await.is_err());
    }

    #[tokio::test]
    async fn cmd_task_schedules_happy_path() {
        let t = FakeTransport::new();
        t.push(Response::SchedulesList { schedules: vec![] });
        assert!(cmd_task_schedules(&t).await.is_ok());
    }

    #[tokio::test]
    async fn cmd_runs_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::RunsList { runs: vec![] });
        assert!(cmd_runs(&t, Some("disk-usage"), 10).await.is_ok());
        assert!(matches!(t.sent()[0], Request::RunsList { .. }));

        let t = FakeTransport::new();
        t.push(Response::Error { message: "bad".into() });
        assert!(cmd_runs(&t, None, 10).await.is_err());
    }

    #[tokio::test]
    async fn cmd_config_list_get_set_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::ConfigEntries { entries: vec![] });
        assert!(cmd_config_list(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::ConfigValue { key: "mimir.model".into(), value: "gpt-4o".into() });
        assert!(cmd_config_get(&t, "mimir.model").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Ok { message: "set".into() });
        assert!(cmd_config_set(&t, "mimir.model", "gpt-4o").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "unknown key".into() });
        assert!(cmd_config_get(&t, "nope").await.is_err());
    }

    #[tokio::test]
    async fn cmd_memory_crud_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::MemoryList { memories: vec![] });
        assert!(cmd_memory_list(&t, Some("fact")).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::MemoryList { memories: vec![] });
        assert!(cmd_memory_search(&t, "postgres").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Ok { message: "saved".into() });
        assert!(cmd_memory_save(&t, "fact", "runs Ubuntu").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Ok { message: "updated".into() });
        assert!(cmd_memory_update(&t, "id1", "new").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Ok { message: "deleted".into() });
        assert!(cmd_memory_delete(&t, "id1").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "not found".into() });
        assert!(cmd_memory_delete(&t, "missing").await.is_err());
    }

    #[tokio::test]
    async fn cmd_memory_export_import_info_reflect() {
        let t = FakeTransport::new();
        t.push(Response::MemoryExport { path: "/tmp/out.db".into() });
        assert!(cmd_memory_export(&t, "/tmp/out.db").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "exists".into() });
        assert!(cmd_memory_import(&t, "/tmp/out.db", false).await.is_err());

        let t = FakeTransport::new();
        t.push(Response::Ok { message: "imported".into() });
        assert!(cmd_memory_import(&t, "/tmp/out.db", true).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::MemoryInfo {
            identity_id: "id1".into(), name: "regin".into(), host: "host1".into(),
            schema_version: "3".into(), memory_count: 4, created_at: "now".into(),
        });
        assert!(cmd_memory_info(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::ReflectStats { episodes: 1, reinforced: 0, created: 1, decayed: 0 });
        assert!(cmd_memory_reflect(&t).await.is_ok());
    }

    #[tokio::test]
    async fn cmd_task_create_happy_no_edit() {
        let t = FakeTransport::new();
        t.push(Response::SkillCreated { path: "/skills/x/skill.md".into(), shadows_system: false });
        assert!(cmd_task_create(&t, "x", None, false, false, false).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "exists".into() });
        assert!(cmd_task_create(&t, "x", None, false, false, false).await.is_err());
    }

    #[tokio::test]
    async fn cmd_context_show_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::Context { repo_key: Some("/repo".into()), content: Some("notes".into()) });
        assert!(cmd_context_show(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "db locked".into() });
        assert!(cmd_context_show(&t).await.is_err());
    }

    #[tokio::test]
    async fn cmd_ok_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::Ok { message: "done".into() });
        assert!(cmd_ok(&t, Request::DesiredCheck).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "boom".into() });
        assert!(cmd_ok(&t, Request::DesiredCheck).await.is_err());
    }

    #[tokio::test]
    async fn cmd_incidents_changes_hypotheses_problems_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::Incidents { incidents: vec![] });
        assert!(cmd_incidents(&t, Request::IncidentList { status: None }).await.is_ok());
        let t = FakeTransport::new();
        t.push(Response::Error { message: "e".into() });
        assert!(cmd_incidents(&t, Request::IncidentList { status: None }).await.is_err());

        let t = FakeTransport::new();
        t.push(Response::Changes { changes: vec![] });
        assert!(cmd_changes(&t, Request::ChangeList).await.is_ok());
        let t = FakeTransport::new();
        t.push(Response::Error { message: "e".into() });
        assert!(cmd_changes(&t, Request::ChangeList).await.is_err());

        let t = FakeTransport::new();
        t.push(Response::Hypotheses { hypotheses: vec![] });
        assert!(cmd_hypotheses(&t, Request::ProblemHypothesisList { problem_id: "p1".into() }).await.is_ok());
        let t = FakeTransport::new();
        t.push(Response::Error { message: "e".into() });
        assert!(cmd_hypotheses(&t, Request::ProblemHypothesisList { problem_id: "p1".into() }).await.is_err());

        let t = FakeTransport::new();
        t.push(Response::Problems { problems: vec![] });
        assert!(cmd_problems(&t, Request::ProblemList { status: None }).await.is_ok());
        let t = FakeTransport::new();
        t.push(Response::Error { message: "e".into() });
        assert!(cmd_problems(&t, Request::ProblemList { status: None }).await.is_err());
    }

    #[tokio::test]
    async fn cmd_desired_list_show_happy_and_error() {
        let t = FakeTransport::new();
        t.push(Response::DesiredListResp { items: vec![] });
        assert!(cmd_desired_list(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::DesiredDetail { state: Box::new(regin_core::desired::DesiredState {
            domain: "disk".into(), intent: "".into(), assertions: vec![],
            recurrence_threshold: None, cadence: None,
            source: regin_core::desired::DesiredSource::System, path: "/x".into(),
        })});
        assert!(cmd_desired_show(&t, "disk").await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "no such domain".into() });
        assert!(cmd_desired_show(&t, "missing").await.is_err());
    }

    #[tokio::test]
    async fn cmd_mode_posture_greeting_audit_checks_filters_metrics() {
        let t = FakeTransport::new();
        t.push(Response::ModeInfo { mode: "standalone".into(), configured: false, last_ok: None, failures: 0 });
        assert!(cmd_mode(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::PostureInfo {
            posture: "conservative".into(), allow_auto: false,
            change_successes: 0, change_failures: 0, change_success_rate: 0.0, promotion_error_rate: 0.0,
        });
        assert!(cmd_posture(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::GreetingResp { greeting: Box::new(regin_core::greeting::Greeting {
            mode: "standalone".into(), open_incidents: 0, open_problems: 0,
            pending_changes: vec![], decision_problems: vec![],
        })});
        assert!(cmd_greeting(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::AuditResult { findings: vec![], trimmed: false, opened: 0 });
        assert!(cmd_audit(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::DerivedChecks { checks: vec![] });
        assert!(cmd_checks(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Filters { rules: vec![] });
        assert!(cmd_filters_list(&t).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Metrics {
            summary: Box::new(regin_core::kpi::KpiSummary {
                since: "now".into(), incidents_opened: 0, incidents_resolved: 0, open_incidents: 0,
                time_in_deviation_secs: 0, mttr_secs: None, recurring_problems: 0,
                remediations_auto: 0, remediations_approved: 0, remediations_escalated: 0,
                automation_ratio: 0.0, autonomy_ratio: 0.0, cost_llm_usd: 0.0, cost_avoided_usd: 0.0,
                notice_filter_saved: 0, promotions: 0, promotion_errors: 0, promotion_error_rate: 0.0,
                change_successes: 0, change_failures: 0, change_success_rate: 0.0,
            }),
            objective: regin_core::kpi::Objective { reliability: 1.0, reliability_floor: 0.95, meets_floor: true, cost_llm_usd: 0.0 },
        });
        assert!(cmd_metrics(&t, Some(30)).await.is_ok());

        let t = FakeTransport::new();
        t.push(Response::Error { message: "e".into() });
        assert!(cmd_mode(&t).await.is_err());
    }

    #[tokio::test]
    async fn cmd_chat_opens_a_conversation_and_exits_cleanly_on_stdin_eof() {
        // cargo test runs with no interactive stdin, so the very first
        // `lines.next()` inside cmd_chat's loop returns None (EOF) and it
        // exits — this exercises the ChatNew + GreetingQuery setup logic
        // without needing a live terminal.
        let t = FakeTransport::new();
        t.push(Response::ChatNew { conversation_id: "c1".into() });
        t.push(Response::GreetingResp { greeting: Box::new(regin_core::greeting::Greeting {
            mode: "standalone".into(), open_incidents: 0, open_problems: 0,
            pending_changes: vec![], decision_problems: vec![],
        })});
        assert!(cmd_chat(&t).await.is_ok());
        assert!(matches!(t.sent()[0], Request::ChatNew));
        assert!(matches!(t.sent()[1], Request::GreetingQuery));
    }

    #[tokio::test]
    async fn cmd_chat_propagates_chat_new_error() {
        let t = FakeTransport::new();
        t.push(Response::Error { message: "daemon down".into() });
        assert!(cmd_chat(&t).await.is_err());
    }

    // -----------------------------------------------------------------
    // Streaming glue (print_chat_event) — logic-only assertions via the
    // pure apply_chat_event it delegates to are covered in render.rs;
    // this confirms the wiring compiles and folds state the same way.
    // -----------------------------------------------------------------

    #[test]
    fn print_chat_event_folds_stream_chunks_into_full_text() {
        let mut full = String::new();
        let mut conv_id = String::new();
        print_chat_event(&Response::StreamChunk { token: "hi".into() }, &mut full, &mut conv_id);
        print_chat_event(&Response::StreamDone { conversation_id: "c9".into() }, &mut full, &mut conv_id);
        assert_eq!(full, "hi");
        assert_eq!(conv_id, "c9");
    }
}
