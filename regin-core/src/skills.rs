use anyhow::{Context, Result, anyhow};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use tracing::{debug, info};

use crate::db;
use crate::llm::NanoGptClient;
use crate::types::{ChatMessage, TaskRun};

/// A skill definition loaded from the skills directory.
#[derive(Debug, Clone)]
pub struct Skill {
    /// The skill name (directory name).
    pub name: String,
    /// A short description (first line of skill.md).
    pub description: String,
    /// The path to the skill directory.
    pub path: std::path::PathBuf,
    /// The full content of skill.md (the main prompt).
    pub prompt: String,
    /// Additional supporting files: (filename, content) pairs.
    pub files: Vec<(String, String)>,
}

/// List all available skills in the skills directory.
///
/// Each skill is a subdirectory containing at least a `skill.md` file.
pub fn list_skills(skills_dir: &Path) -> Result<Vec<Skill>> {
    if !skills_dir.exists() {
        debug!("Skills directory does not exist: {}", skills_dir.display());
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();

    let entries = fs::read_dir(skills_dir)
        .with_context(|| format!("Failed to read skills directory: {}", skills_dir.display()))?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let skill_md = path.join("skill.md");
        if !skill_md.exists() {
            debug!("Skipping directory without skill.md: {}", path.display());
            continue;
        }

        match load_skill_from_path(&path) {
            Ok(skill) => skills.push(skill),
            Err(e) => {
                tracing::warn!("Failed to load skill from {}: {}", path.display(), e);
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    debug!(count = skills.len(), "Skills listed");
    Ok(skills)
}

/// Load a specific skill by name from the skills directory.
pub fn load_skill(skills_dir: &Path, name: &str) -> Result<Skill> {
    let skill_path = skills_dir.join(name);
    if !skill_path.exists() {
        return Err(anyhow!("Skill '{}' not found at {}", name, skill_path.display()));
    }
    if !skill_path.is_dir() {
        return Err(anyhow!("Skill '{}' is not a directory", name));
    }
    load_skill_from_path(&skill_path)
}

/// Load a skill from its directory path.
fn load_skill_from_path(path: &Path) -> Result<Skill> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .context("Invalid skill directory name")?;

    let skill_md_path = path.join("skill.md");
    let prompt = fs::read_to_string(&skill_md_path)
        .with_context(|| format!("Failed to read skill.md: {}", skill_md_path.display()))?;

    let description = prompt
        .lines()
        .next()
        .unwrap_or("")
        .trim_start_matches('#')
        .trim()
        .to_string();

    // Load any supporting files (everything except skill.md)
    let mut files = Vec::new();
    let entries = fs::read_dir(path)
        .with_context(|| format!("Failed to read skill directory: {}", path.display()))?;

    for entry in entries {
        let entry = entry.context("Failed to read skill file entry")?;
        let file_path = entry.path();

        if !file_path.is_file() {
            continue;
        }

        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if filename == "skill.md" {
            continue;
        }

        // Try to read as text; skip binary files
        match fs::read_to_string(&file_path) {
            Ok(content) => {
                files.push((filename, content));
            }
            Err(_) => {
                debug!("Skipping non-text file: {}", file_path.display());
            }
        }
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    debug!(skill = %name, files = files.len(), "Skill loaded");

    Ok(Skill {
        name,
        description,
        path: path.to_path_buf(),
        prompt,
        files,
    })
}

/// Run a skill: send the skill prompt (plus any supporting file contents) to the LLM,
/// and save the result as a task run in the database.
pub async fn run_skill(
    skill: &Skill,
    llm_client: &NanoGptClient,
    db_conn: &Connection,
) -> Result<TaskRun> {
    info!(skill = %skill.name, "Running skill");

    let started_at = chrono::Utc::now().to_rfc3339();

    // Build the messages: system prompt from skill.md, then supporting files as context
    let mut user_content = skill.prompt.clone();

    if !skill.files.is_empty() {
        user_content.push_str("\n\n--- Supporting Files ---\n");
        for (filename, content) in &skill.files {
            user_content.push_str(&format!("\n### {}\n```\n{}\n```\n", filename, content));
        }
    }

    let messages = vec![ChatMessage::user(user_content)];

    let (status, output) = match llm_client.chat_completion(&messages).await {
        Ok(response) => ("success".to_string(), response),
        Err(e) => {
            let err_msg = format!("Skill execution failed: {}", e);
            tracing::error!(skill = %skill.name, error = %e, "Skill execution failed");
            ("error".to_string(), err_msg)
        }
    };

    let finished_at = chrono::Utc::now().to_rfc3339();

    let task_run = db::save_task_run(db_conn, &skill.name, &status, &output, &started_at, &finished_at)?;

    info!(
        skill = %skill.name,
        status = %task_run.status,
        "Skill run completed"
    );

    Ok(task_run)
}
