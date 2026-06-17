use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Application configuration loaded from `~/.config/regin/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub nanogpt_api_key: String,
    pub nanogpt_model: String,
    pub nanogpt_base_url: String,
    pub db_path: String,
    pub skills_dir: String,
    pub schedule_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            nanogpt_api_key: String::new(),
            nanogpt_model: "claude-sonnet-4-20250514".to_string(),
            nanogpt_base_url: "https://nano-gpt.com/api/v1".to_string(),
            db_path: "~/.config/regin/regin.db".to_string(),
            skills_dir: "~/.config/regin/skills".to_string(),
            schedule_interval_secs: 3600,
        }
    }
}

impl Config {
    /// Returns the default config file path: `~/.config/regin/config.toml`.
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?;
        Ok(config_dir.join("regin").join("config.toml"))
    }

    /// System-wide config path: /etc/regin/config.toml
    pub fn system_path() -> PathBuf {
        PathBuf::from("/etc/regin/config.toml")
    }

    /// Load configuration, checking paths in order:
    /// 1. ~/.config/regin/config.toml (if it exists)
    /// 2. /etc/regin/config.toml (if it exists)
    /// 3. Create default at ~/.config/regin/config.toml
    pub fn load() -> Result<Self> {
        let user_path = Self::default_path()?;
        if user_path.exists() {
            return Self::load_from(&user_path);
        }
        let sys_path = Self::system_path();
        if sys_path.exists() {
            return Self::load_from(&sys_path);
        }
        // Neither exists — create default user config
        Self::load_from(&user_path)
    }

    /// Load configuration from a specific file path. If the file doesn't exist,
    /// a default config is written to that path and returned.
    pub fn load_from(path: &Path) -> Result<Self> {
        if path.exists() {
            debug!("Loading config from {}", path.display());
            let contents = fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            let config: Config = toml::from_str(&contents)
                .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
            Ok(config)
        } else {
            info!("Config file not found at {}, creating default", path.display());
            let config = Config::default();
            config.save_to(path)?;
            Ok(config)
        }
    }

    /// Save this configuration to the default path.
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path()?;
        self.save_to(&path)
    }

    /// Save this configuration to a specific file path.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }
        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(path, contents)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;
        debug!("Config saved to {}", path.display());
        Ok(())
    }

    /// Expand a path that may start with `~` to use the actual home directory.
    pub fn expand_path(path: &str) -> PathBuf {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        PathBuf::from(path)
    }

    /// Get the expanded database path.
    pub fn db_path_expanded(&self) -> PathBuf {
        Self::expand_path(&self.db_path)
    }

    /// Get the expanded skills directory path.
    pub fn skills_dir_expanded(&self) -> PathBuf {
        Self::expand_path(&self.skills_dir)
    }
}
