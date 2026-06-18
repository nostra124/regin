use anyhow::{Context, Result, anyhow};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::db;
use crate::llm::NanoGptClient;
use crate::types::{ChatMessage, TaskRun};

/// A skill definition loaded from disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: std::path::PathBuf,
    pub prompt: String,
    /// Whether this skill comes from the system dir or user dir.
    pub source: SkillSource,
    pub files: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillSource {
    System,
    User,
}

impl std::fmt::Display for SkillSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillSource::System => write!(f, "system"),
            SkillSource::User => write!(f, "user"),
        }
    }
}

/// List all skills, merging system and user directories.
/// User skills override system skills with the same name.
pub fn list_all_skills(system_dir: &Path, user_dir: &Path) -> Result<Vec<Skill>> {
    let mut by_name: BTreeMap<String, Skill> = BTreeMap::new();

    // Load system skills first
    for skill in load_skills_from_dir(system_dir, SkillSource::System)? {
        by_name.insert(skill.name.clone(), skill);
    }

    // User skills override system
    for skill in load_skills_from_dir(user_dir, SkillSource::User)? {
        by_name.insert(skill.name.clone(), skill);
    }

    let skills: Vec<Skill> = by_name.into_values().collect();
    debug!(count = skills.len(), "All skills listed (system + user)");
    Ok(skills)
}

/// Load skills from a single directory.
fn load_skills_from_dir(dir: &Path, source: SkillSource) -> Result<Vec<Skill>> {
    if !dir.exists() {
        debug!("Skills directory does not exist: {}", dir.display());
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read skills directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("skill.md");
        if !skill_md.exists() {
            continue;
        }
        match load_skill_from_path(&path, source.clone()) {
            Ok(skill) => skills.push(skill),
            Err(e) => {
                tracing::warn!("Failed to load skill from {}: {}", path.display(), e);
            }
        }
    }
    Ok(skills)
}

/// Load a specific skill by name, checking user dir first then system dir.
pub fn load_skill(system_dir: &Path, user_dir: &Path, name: &str) -> Result<Skill> {
    // User overrides system
    let user_path = user_dir.join(name);
    if user_path.is_dir() && user_path.join("skill.md").exists() {
        return load_skill_from_path(&user_path, SkillSource::User);
    }
    let sys_path = system_dir.join(name);
    if sys_path.is_dir() && sys_path.join("skill.md").exists() {
        return load_skill_from_path(&sys_path, SkillSource::System);
    }
    Err(anyhow!("Skill '{}' not found in user or system skills", name))
}

fn load_skill_from_path(path: &Path, source: SkillSource) -> Result<Skill> {
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
        match fs::read_to_string(&file_path) {
            Ok(content) => files.push((filename, content)),
            Err(_) => {
                debug!("Skipping non-text file: {}", file_path.display());
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    debug!(skill = %name, source = %source, files = files.len(), "Skill loaded");

    Ok(Skill {
        name,
        description,
        path: path.to_path_buf(),
        prompt,
        source,
        files,
    })
}

/// Run a skill: send the skill prompt (plus supporting files) to the LLM,
/// save the result as a task run in the database.
pub async fn run_skill(
    skill: &Skill,
    llm_client: &NanoGptClient,
    db_conn: &Connection,
) -> Result<TaskRun> {
    info!(skill = %skill.name, source = %skill.source, "Running skill");

    let started_at = chrono::Utc::now().to_rfc3339();

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

    info!(skill = %skill.name, status = %task_run.status, "Skill run completed");
    Ok(task_run)
}

// ---------------------------------------------------------------------------
// Skill authoring (FEAT-007)
// ---------------------------------------------------------------------------

/// A starter `skill.md`. The first line is the description shown in `task list`.
pub fn skill_template(name: &str) -> String {
    format!(
        "{name}: one-line description of what this skill does\n\n\
         You are running the `{name}` operational task. Replace this with the\n\
         instructions the agent should follow. The first line above is the skill\n\
         description shown in `regin task list`.\n"
    )
}

/// Whether a system skill of this name exists (used to warn about shadowing).
pub fn system_skill_exists(system_dir: &Path, name: &str) -> bool {
    system_dir.join(name).join("skill.md").exists()
}

/// Write a user skill `<user_dir>/<name>/skill.md`. Refuses to overwrite an
/// existing user skill unless `force`. Returns the path written.
pub fn create_skill(user_dir: &Path, name: &str, content: &str, force: bool) -> Result<PathBuf> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(anyhow!("Invalid skill name: {name:?}"));
    }
    let dir = user_dir.join(name);
    let skill_md = dir.join("skill.md");
    if skill_md.exists() && !force {
        return Err(anyhow!(
            "Skill '{name}' already exists at {} (use --force to overwrite)",
            skill_md.display()
        ));
    }
    fs::create_dir_all(&dir).with_context(|| format!("create dir {}", dir.display()))?;
    fs::write(&skill_md, content).with_context(|| format!("write {}", skill_md.display()))?;
    info!(skill = name, "Skill created");
    Ok(skill_md)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("regin-skill-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn create_scaffold_then_it_loads() {
        let user = tmpdir();
        let path = create_skill(&user, "disk-trend", &skill_template("disk-trend"), false).unwrap();
        assert!(path.exists());
        // it is discoverable as a user skill, with a parsed description
        let sys = tmpdir();
        let listed = list_all_skills(&sys, &user).unwrap();
        let s = listed.iter().find(|s| s.name == "disk-trend").expect("listed");
        assert_eq!(s.source, SkillSource::User);
        assert!(s.description.starts_with("disk-trend:"));
        std::fs::remove_dir_all(&user).ok();
        std::fs::remove_dir_all(&sys).ok();
    }

    #[test]
    fn overwrite_is_guarded_unless_forced() {
        let user = tmpdir();
        create_skill(&user, "x", "first", false).unwrap();
        assert!(create_skill(&user, "x", "second", false).is_err(), "must refuse overwrite");
        create_skill(&user, "x", "second", true).unwrap();
        assert_eq!(std::fs::read_to_string(user.join("x").join("skill.md")).unwrap(), "second");
        std::fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn invalid_names_rejected() {
        let user = tmpdir();
        assert!(create_skill(&user, "a/b", "c", false).is_err());
        assert!(create_skill(&user, "..", "c", false).is_err());
        assert!(create_skill(&user, "", "c", false).is_err());
        std::fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn system_shadow_detected() {
        let sys = tmpdir();
        create_skill(&sys, "shadowme", "sys", false).unwrap();
        assert!(system_skill_exists(&sys, "shadowme"));
        assert!(!system_skill_exists(&sys, "nope"));
        std::fs::remove_dir_all(&sys).ok();
    }
}
