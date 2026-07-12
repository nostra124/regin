//! FEAT-011 (DISC-005): role personas. A regin instance *becomes* a role — a CFO,
//! a dev-lead, a cave foreman. A persona declares the role id, a system-prompt
//! preamble that shapes the agent's behaviour, and an allowed **capability
//! ceiling**: the set of tools the persona may use. The tool dispatcher refuses
//! anything outside that ceiling (least privilege / authorization ceiling).
//!
//! The persona is loaded from a `persona.toml` (path in `REGIN_PERSONA`). With no
//! persona configured, regin runs unscoped (all tools) — the standalone default.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The tools regin can hold a ceiling over (mirrors `tools::execute_tool`).
pub const ALL_TOOLS: &[&str] = &["bash", "read_file", "write_file", "edit_file", "web_search"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Persona {
    /// Role id (e.g. `cfo`, `dev-lead`, `foreman`).
    pub role: String,
    #[serde(default)]
    pub title: String,
    /// System-prompt preamble injected ahead of the base prompt.
    #[serde(default)]
    pub prompt: String,
    /// The capability ceiling: tools this persona may use. Empty = unscoped
    /// (all tools allowed) — the standalone default.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Per-Persona decision-mode override (`"act"` | `"deliberate"`, FEAT-028 /
    /// DISC-018). Unset = no override; the risk classifier decides.
    #[serde(default)]
    pub default_mode: Option<String>,
}

impl Persona {
    pub fn from_toml(s: &str) -> Result<Persona> {
        let p: Persona = toml::from_str(s).context("parsing persona.toml")?;
        p.validate()?;
        Ok(p)
    }

    /// Load the persona from `REGIN_PERSONA`, or `None` when unset (unscoped).
    pub fn from_env() -> Result<Option<Persona>> {
        match std::env::var("REGIN_PERSONA") {
            Ok(path) if !path.is_empty() => {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading persona {path}"))?;
                Ok(Some(Persona::from_toml(&text)?))
            }
            _ => Ok(None),
        }
    }

    fn validate(&self) -> Result<()> {
        if self.role.trim().is_empty() {
            anyhow::bail!("persona has an empty role");
        }
        for t in &self.tools {
            if !ALL_TOOLS.contains(&t.as_str()) {
                anyhow::bail!("persona {:?}: unknown tool {t:?} (known: {ALL_TOOLS:?})", self.role);
            }
        }
        if let Some(m) = &self.default_mode
            && m != "act" && m != "deliberate"
        {
            anyhow::bail!("persona {:?}: default_mode must be \"act\" or \"deliberate\", got {m:?}", self.role);
        }
        Ok(())
    }

    /// Whether `tool` is within this persona's ceiling. An empty ceiling allows
    /// everything (unscoped).
    pub fn allows(&self, tool: &str) -> bool {
        self.tools.is_empty() || self.tools.iter().any(|t| t == tool)
    }
}

/// The capability ceiling for an optional persona: `None` (no persona) allows
/// every tool. Centralizes the "unscoped default" rule so callers don't repeat it.
pub fn allows(persona: Option<&Persona>, tool: &str) -> bool {
    match persona {
        Some(p) => p.allows(tool),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_round_trips() {
        let p = Persona::from_toml(
            "role = \"cfo\"\ntitle = \"Chief Financial Officer\"\nprompt = \"You steward the budget.\"\ntools = [\"bash\", \"read_file\"]\n",
        )
        .unwrap();
        assert_eq!(p.role, "cfo");
        assert_eq!(p.title, "Chief Financial Officer");
        assert!(p.prompt.contains("budget"));
        assert_eq!(p.tools, vec!["bash", "read_file"]);
    }

    #[test]
    fn ceiling_allows_listed_and_refuses_others() {
        let p = Persona::from_toml("role = \"dev-lead\"\ntools = [\"bash\", \"read_file\", \"edit_file\"]\n").unwrap();
        assert!(p.allows("bash"));
        assert!(p.allows("edit_file"));
        assert!(!p.allows("web_search"), "outside the ceiling");
        assert!(!p.allows("write_file"));
    }

    #[test]
    fn empty_ceiling_is_unscoped() {
        let p = Persona::from_toml("role = \"foreman\"\n").unwrap();
        assert!(p.allows("bash") && p.allows("web_search"), "empty tools = all allowed");
        // and the None case
        assert!(allows(None, "web_search"));
        assert!(allows(Some(&p), "write_file"));
    }

    #[test]
    fn validation_rejects_empty_role_and_unknown_tool() {
        assert!(Persona::from_toml("role = \"\"\n").is_err());
        assert!(Persona::from_toml("role = \"x\"\ntools = [\"telepathy\"]\n").is_err());
    }

    #[test]
    fn default_mode_is_optional_and_validated() {
        let p = Persona::from_toml("role = \"x\"\n").unwrap();
        assert_eq!(p.default_mode, None);
        let p = Persona::from_toml("role = \"x\"\ndefault_mode = \"deliberate\"\n").unwrap();
        assert_eq!(p.default_mode.as_deref(), Some("deliberate"));
        assert!(Persona::from_toml("role = \"x\"\ndefault_mode = \"whenever\"\n").is_err());
    }
}
