use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use futures_util::StreamExt;
use regin_core::{
    config::Config,
    db,
    llm::NanoGptClient,
    skills,
    types::ChatMessage,
};
use rusqlite::Connection;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "regin", version, about = "Regin – your personal AI agent")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive chat with the LLM
    Chat,
    /// Manage skills
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// Show recent task runs
    Runs {
        /// Filter by skill name
        #[arg(long)]
        skill: Option<String>,
        /// Maximum number of runs to show
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Show current configuration
    Config,
}

#[derive(Subcommand)]
enum SkillAction {
    /// List all available skills
    List,
    /// Run a skill by name
    Run {
        /// Skill name
        name: String,
    },
    /// Show skill details (prompt + supporting files)
    Show {
        /// Skill name
        name: String,
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
        Commands::Skill { action } => match action {
            SkillAction::List => cmd_skill_list(),
            SkillAction::Run { name } => cmd_skill_run(&name).await,
            SkillAction::Show { name } => cmd_skill_show(&name),
        },
        Commands::Runs { skill, limit } => cmd_runs(skill.as_deref(), limit),
        Commands::Config => cmd_config(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn load_cfg() -> Result<Config> {
    Config::load().context("Failed to load configuration")
}

fn open_db(cfg: &Config) -> Result<Connection> {
    db::init_db(&cfg.db_path_expanded()).context("Failed to initialise database")
}

fn make_client(cfg: &Config) -> NanoGptClient {
    NanoGptClient::new(&cfg.nanogpt_base_url, &cfg.nanogpt_api_key, &cfg.nanogpt_model)
}

/// Print coloured text to stdout (inline, no newline).
fn print_color(text: &str, color: Color) {
    let mut out = io::stdout();
    let _ = crossterm::execute!(out, SetForegroundColor(color));
    let _ = write!(out, "{text}");
    let _ = crossterm::execute!(out, ResetColor);
}

/// Print coloured line to stdout.
fn println_color(text: &str, color: Color) {
    print_color(text, color);
    println!();
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn cmd_chat() -> Result<()> {
    let cfg = load_cfg()?;
    let conn = open_db(&cfg)?;
    let client = make_client(&cfg);

    println_color(
        "Welcome to regin chat! Commands: /new  /history  /quit",
        Color::Yellow,
    );
    println_color(
        &format!("Model: {}", cfg.nanogpt_model),
        Color::DarkGrey,
    );
    println!();

    let mut conversation_id = new_id();
    let mut conversation_title = String::from("New conversation");
    let mut history: Vec<ChatMessage> = Vec::new();

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        // Prompt
        print_color("you> ", Color::Cyan);
        io::stdout().flush()?;

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break, // EOF / Ctrl-D
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Slash commands
        match trimmed {
            "/quit" | "/exit" => break,
            "/new" => {
                conversation_id = new_id();
                conversation_title = String::from("New conversation");
                history.clear();
                println_color("— started new conversation —", Color::Yellow);
                println!();
                continue;
            }
            "/history" => {
                let convos = db::list_conversations(&conn)?;
                if convos.is_empty() {
                    println_color("No conversations yet.", Color::Yellow);
                } else {
                    println_color("Recent conversations:", Color::Yellow);
                    for c in convos.iter().take(20) {
                        println!("  {} | {} | {}", &c.id[..8], c.updated_at, c.title);
                    }
                }
                println!();
                continue;
            }
            _ => {}
        }

        // First user message becomes the title
        if history.is_empty() {
            conversation_title = trimmed.chars().take(80).collect();
        }

        // Save & record user message
        let user_msg = ChatMessage::user(trimmed);
        history.push(user_msg);
        db::save_message(&conn, &conversation_id, &conversation_title, "user", trimmed)?;

        // Stream assistant response
        print_color("assistant> ", Color::Green);
        io::stdout().flush()?;

        let mut full_response = String::new();

        match client.chat_completion_stream(&history).await {
            Ok(mut stream) => {
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(token) => {
                            print_color(&token, Color::Green);
                            io::stdout().flush()?;
                            full_response.push_str(&token);
                        }
                        Err(e) => {
                            println_color(&format!("\n[stream error: {e}]"), Color::Red);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                println_color(&format!("[error: {e}]"), Color::Red);
                println!();
                continue;
            }
        }

        println!(); // newline after streamed response
        println!(); // blank line for readability

        // Save assistant message
        if !full_response.is_empty() {
            history.push(ChatMessage::assistant(&full_response));
            db::save_message(
                &conn,
                &conversation_id,
                &conversation_title,
                "assistant",
                &full_response,
            )?;
        }
    }

    println_color("Goodbye!", Color::Yellow);
    Ok(())
}

fn cmd_skill_list() -> Result<()> {
    let cfg = load_cfg()?;
    let skills = skills::list_skills(&cfg.skills_dir_expanded())?;

    if skills.is_empty() {
        println!("No skills found in {}", cfg.skills_dir);
        return Ok(());
    }

    println_color(
        &format!("Skills ({}):", skills.len()),
        Color::Yellow,
    );
    for s in &skills {
        println_color(&format!("  {}", s.name), Color::Cyan);
        println!("    {}", s.description);
    }
    Ok(())
}

async fn cmd_skill_run(name: &str) -> Result<()> {
    let cfg = load_cfg()?;
    let conn = open_db(&cfg)?;
    let client = make_client(&cfg);
    let skill = skills::load_skill(&cfg.skills_dir_expanded(), name)?;

    println_color(&format!("Running skill '{name}'…"), Color::Yellow);

    let run = skills::run_skill(&skill, &client, &conn).await?;

    println_color(&format!("Status: {}", run.status), Color::Cyan);
    println!();
    println!("{}", run.output);
    Ok(())
}

fn cmd_skill_show(name: &str) -> Result<()> {
    let cfg = load_cfg()?;
    let skill = skills::load_skill(&cfg.skills_dir_expanded(), name)?;

    println_color(&format!("Skill: {}", skill.name), Color::Cyan);
    println_color(&format!("Path:  {}", skill.path.display()), Color::DarkGrey);
    println!();
    println_color("— skill.md —", Color::Yellow);
    println!("{}", skill.prompt);

    if !skill.files.is_empty() {
        println_color(
            &format!("Supporting files ({}):", skill.files.len()),
            Color::Yellow,
        );
        for (fname, _content) in &skill.files {
            println!("  • {fname}");
        }
    }
    Ok(())
}

fn cmd_runs(skill: Option<&str>, limit: usize) -> Result<()> {
    let cfg = load_cfg()?;
    let conn = open_db(&cfg)?;

    // The core API requires a skill name; if none given list all skills then
    // fetch for each.  For simplicity, when no filter we query with "%" using
    // a direct SQL fallback.
    let runs = if let Some(name) = skill {
        db::get_task_runs(&conn, name, limit)?
    } else {
        // Fetch all runs (small helper query)
        let mut stmt = conn.prepare(
            "SELECT id, skill_name, status, output, started_at, finished_at \
             FROM task_runs ORDER BY started_at DESC LIMIT ?1",
        )?;
        stmt.query_map([limit as i64], |row| {
            Ok(regin_core::types::TaskRun {
                id: row.get(0)?,
                skill_name: row.get(1)?,
                status: row.get(2)?,
                output: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
    };

    if runs.is_empty() {
        println!("No task runs found.");
        return Ok(());
    }

    println_color(
        &format!("Task runs ({}):", runs.len()),
        Color::Yellow,
    );
    for r in &runs {
        let status_color = if r.status == "success" {
            Color::Green
        } else {
            Color::Red
        };
        print!("  {} | {} | ", r.started_at, r.skill_name);
        println_color(&r.status, status_color);
        // Show first line of output
        if let Some(first_line) = r.output.lines().next() {
            let preview: String = first_line.chars().take(100).collect();
            println_color(&format!("    {preview}"), Color::DarkGrey);
        }
    }
    Ok(())
}

fn cmd_config() -> Result<()> {
    let cfg = load_cfg()?;

    let redacted_key = if cfg.nanogpt_api_key.len() > 8 {
        format!("{}…{}", &cfg.nanogpt_api_key[..4], &cfg.nanogpt_api_key[cfg.nanogpt_api_key.len() - 4..])
    } else if cfg.nanogpt_api_key.is_empty() {
        "(not set)".into()
    } else {
        "****".into()
    };

    println_color("Current configuration:", Color::Yellow);
    println!("  API key:       {redacted_key}");
    println!("  Model:         {}", cfg.nanogpt_model);
    println!("  Base URL:      {}", cfg.nanogpt_base_url);
    println!("  DB path:       {}", cfg.db_path);
    println!("  Skills dir:    {}", cfg.skills_dir);
    println!("  Schedule (s):  {}", cfg.schedule_interval_secs);
    Ok(())
}
