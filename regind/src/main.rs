use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info, warn};

/// regind — the Regin monitoring daemon.
///
/// Runs scheduled skills on a configurable interval, logging results
/// to SQLite. Designed to be managed by systemd.
#[derive(Parser, Debug)]
#[command(name = "regind", about = "Regin monitoring daemon")]
struct Cli {
    /// Path to a config file (default: ~/.config/regin/config.toml)
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Run all skills once and exit (useful for testing)
    #[arg(long)]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing (respects RUST_LOG, defaults to "info")
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    info!("regind starting up");

    // Load configuration
    let config = match &cli.config {
        Some(path) => regin_core::config::Config::load_from(path)
            .with_context(|| format!("Failed to load config from {}", path.display()))?,
        None => regin_core::config::Config::load()
            .context("Failed to load config from default location")?,
    };

    info!(
        model = %config.nanogpt_model,
        db_path = %config.db_path,
        skills_dir = %config.skills_dir,
        interval_secs = config.schedule_interval_secs,
        "Configuration loaded"
    );

    // Initialize database
    let db_path = config.db_path_expanded();
    let conn = regin_core::db::init_db(&db_path)
        .with_context(|| format!("Failed to initialize database at {}", db_path.display()))?;

    info!("Database initialized at {}", db_path.display());

    // Build LLM client
    let llm_client = regin_core::llm::NanoGptClient::new(
        &config.nanogpt_base_url,
        &config.nanogpt_api_key,
        &config.nanogpt_model,
    );

    // Discover skills
    let skills_dir = config.skills_dir_expanded();
    let skills = regin_core::skills::list_skills(&skills_dir)
        .with_context(|| format!("Failed to list skills from {}", skills_dir.display()))?;

    if skills.is_empty() {
        warn!("No skills found in {}", skills_dir.display());
    } else {
        info!("Found {} skill(s):", skills.len());
        for skill in &skills {
            info!("  - {} : {}", skill.name, skill.description);
        }
    }

    // --once mode: run all skills once and exit
    if cli.once {
        info!("Running all skills once (--once mode)");
        run_all_skills(&skills, &llm_client, &conn).await;
        info!("All skills completed, exiting");
        return Ok(());
    }

    // Main loop: run skills on the configured interval, with graceful shutdown
    let interval_duration = tokio::time::Duration::from_secs(config.schedule_interval_secs);
    info!(
        "Entering main loop (interval: {}s). Send SIGTERM/SIGINT to stop.",
        config.schedule_interval_secs
    );

    // Run skills immediately on startup, then wait for the interval
    run_all_skills(&skills, &llm_client, &conn).await;

    let mut interval = tokio::time::interval(interval_duration);
    // The first tick completes immediately; consume it since we already ran above.
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                info!("Scheduled run starting");
                run_all_skills(&skills, &llm_client, &conn).await;
                info!("Scheduled run complete");
            }
            _ = shutdown_signal() => {
                info!("Shutdown signal received, exiting gracefully");
                break;
            }
        }
    }

    info!("regind stopped");
    Ok(())
}

/// Run every skill, logging successes and failures.
async fn run_all_skills(
    skills: &[regin_core::skills::Skill],
    llm_client: &regin_core::llm::NanoGptClient,
    conn: &rusqlite::Connection,
) {
    for skill in skills {
        match regin_core::skills::run_skill(skill, llm_client, conn).await {
            Ok(task_run) => {
                info!(
                    skill = %task_run.skill_name,
                    status = %task_run.status,
                    output_len = task_run.output.len(),
                    "Skill completed"
                );
            }
            Err(e) => {
                error!(skill = %skill.name, error = %e, "Skill run failed");
            }
        }
    }
}

/// Wait for either SIGTERM or SIGINT (Ctrl-C).
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
