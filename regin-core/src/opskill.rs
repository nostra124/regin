//! Operator-skill format (FEAT-045 / DISC-012).
//!
//! An operator skill ↔ one to-be-state domain, bundling a **monitor** (a command
//! that emits observed signals), an implicit link to the domain's to-be-state file
//! (FEAT-033, loaded separately), and a **remediation playbook** whose entries are
//! each tagged for a DISC-009 lane. The skills engine runs the monitor, evaluates
//! observed-vs-target (FEAT-034), and offers the playbook to the guardrail
//! (FEAT-037). Manifests are TOML, layered user-over-system like skills, and
//! authorable via the FEAT-007 creation flow.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::desired::AssertValue;
use crate::remediation::{self, CandidateFix, RiskClass};

/// Where an operator skill came from (user overrides system).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpSkillSource {
    System,
    User,
}

/// One remediation in a skill's playbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemediationSpec {
    /// A pre-blessed safe-action tag (FEAT-037), if this is a known reversible op.
    pub tag: Option<String>,
    pub title: String,
    pub description: String,
    /// The command that performs the fix.
    pub command: String,
    /// Explicit risk for a non-pre-blessed fix: safe|uncertain|out_of_control.
    pub risk: Option<String>,
}

impl RemediationSpec {
    /// Map to a [`CandidateFix`] for the remediation engine: a pre-blessed tag is
    /// Safe + reversible; otherwise the declared risk (default uncertain), with
    /// `reversible` true only for an explicit `safe`.
    pub fn to_candidate_fix(&self) -> CandidateFix {
        if let Some(tag) = &self.tag
            && let Some(fix) = remediation::from_preblessed(tag, &self.title, &self.description)
        {
            return fix;
        }
        let risk = match self.risk.as_deref().map(|s| s.trim().to_lowercase()).as_deref() {
            Some("safe") => RiskClass::Safe,
            Some("out_of_control") | Some("escalate") => RiskClass::OutOfControl,
            _ => RiskClass::Uncertain,
        };
        CandidateFix {
            title: self.title.clone(),
            description: self.description.clone(),
            reversible: matches!(risk, RiskClass::Safe),
            risk,
        }
    }
}

/// A parsed operator skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorSkill {
    pub domain: String,
    /// Command emitting `key=value` observed-signal lines on stdout.
    pub monitor_command: String,
    #[serde(default, rename = "remediation")]
    pub remediations: Vec<RemediationSpec>,
    #[serde(skip)]
    pub source: Option<OpSkillSource>,
}

/// Parse an operator-skill manifest (TOML).
pub fn parse(content: &str) -> Result<OperatorSkill> {
    let mut skill: OperatorSkill =
        toml::from_str(content).context("parsing operator-skill manifest")?;
    if skill.domain.trim().is_empty() {
        bail!("operator skill has an empty domain");
    }
    if skill.monitor_command.trim().is_empty() {
        bail!("operator skill `{}` has an empty monitor_command", skill.domain);
    }
    skill.source = None;
    Ok(skill)
}

/// Parse monitor output (`key=value` per line) into observed signals. A value that
/// parses as a number becomes `Num`, else `Text`. Blank/`#` lines are ignored.
pub fn parse_signals(stdout: &str) -> BTreeMap<String, AssertValue> {
    let mut out = BTreeMap::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            if k.is_empty() {
                continue;
            }
            let value = match v.parse::<f64>() {
                Ok(n) => AssertValue::Num(n),
                Err(_) => AssertValue::Text(v.to_string()),
            };
            out.insert(k.to_string(), value);
        }
    }
    out
}

fn load_from_dir(dir: &Path, source: OpSkillSource) -> Vec<OperatorSkill> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        match fs::read_to_string(&path).map_err(anyhow::Error::from).and_then(|c| parse(&c)) {
            Ok(mut s) => {
                s.source = Some(source);
                out.push(s);
            }
            Err(e) => tracing::warn!("skipping malformed operator skill {}: {e:#}", path.display()),
        }
    }
    out
}

/// Load all operator skills, user overriding system by domain (fail-safe).
pub fn load_all(system_dir: &Path, user_dir: &Path) -> Vec<OperatorSkill> {
    let mut by_domain: BTreeMap<String, OperatorSkill> = BTreeMap::new();
    for s in load_from_dir(system_dir, OpSkillSource::System) {
        by_domain.insert(s.domain.clone(), s);
    }
    for s in load_from_dir(user_dir, OpSkillSource::User) {
        by_domain.insert(s.domain.clone(), s);
    }
    by_domain.into_values().collect()
}

/// A starter operator-skill manifest for the FEAT-007 authoring flow.
pub fn template(domain: &str) -> String {
    format!(
        "domain = \"{domain}\"\n\
         # Emit observed signals as key=value lines on stdout.\n\
         monitor_command = \"echo {domain}.example=0\"\n\
         \n\
         # Remediation playbook (each fix is routed to a DISC-009 lane):\n\
         # [[remediation]]\n\
         # tag = \"clear_temp\"      # a pre-blessed reversible op auto-qualifies as safe\n\
         # title = \"...\"\n\
         # description = \"...\"\n\
         # command = \"...\"\n\
         # # or, for a novel fix: risk = \"uncertain\"\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("regin-opskill-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    const DISK: &str = "\
domain = \"disk\"
monitor_command = \"df --output=pcent / | tail -1 | tr -dc 0-9 | xargs -I{} echo disk.root.use_percent={}\"

[[remediation]]
tag = \"clear_temp\"
title = \"clear /tmp\"
description = \"remove temp files\"
command = \"rm -rf /tmp/*\"

[[remediation]]
title = \"edit logging config\"
description = \"reduce log verbosity\"
command = \"sed -i ...\"
risk = \"uncertain\"
";

    #[test]
    fn parses_manifest_and_playbook() {
        let s = parse(DISK).unwrap();
        assert_eq!(s.domain, "disk");
        assert!(s.monitor_command.contains("df"));
        assert_eq!(s.remediations.len(), 2);
        assert_eq!(s.remediations[0].tag.as_deref(), Some("clear_temp"));
    }

    #[test]
    fn validation_rejects_empty_domain_or_monitor() {
        assert!(parse("domain = \"\"\nmonitor_command = \"x\"\n").is_err());
        assert!(parse("domain = \"d\"\nmonitor_command = \"\"\n").is_err());
        assert!(parse("not valid toml ===").is_err());
    }

    #[test]
    fn remediation_maps_to_lane_inputs() {
        let s = parse(DISK).unwrap();
        // pre-blessed tag -> Safe + reversible
        let f0 = s.remediations[0].to_candidate_fix();
        assert_eq!(f0.risk, RiskClass::Safe);
        assert!(f0.reversible);
        // explicit uncertain -> Uncertain + not reversible
        let f1 = s.remediations[1].to_candidate_fix();
        assert_eq!(f1.risk, RiskClass::Uncertain);
        assert!(!f1.reversible);
        // explicit out_of_control
        let spec = RemediationSpec { tag: None, title: "t".into(), description: "d".into(), command: "c".into(), risk: Some("out_of_control".into()) };
        assert_eq!(spec.to_candidate_fix().risk, RiskClass::OutOfControl);
    }

    #[test]
    fn parses_signals_num_and_text() {
        let sig = parse_signals("disk.root.use_percent=87\n# comment\n\ndisk.fs.mode=rw\nbad line\n");
        assert_eq!(sig.len(), 2);
        assert_eq!(sig.get("disk.root.use_percent"), Some(&AssertValue::Num(87.0)));
        assert_eq!(sig.get("disk.fs.mode"), Some(&AssertValue::Text("rw".into())));
    }

    #[test]
    fn user_overrides_system_by_domain() {
        let sys = tmpdir();
        let user = tmpdir();
        fs::write(sys.join("disk.toml"), DISK).unwrap();
        fs::write(sys.join("net.toml"), "domain=\"net\"\nmonitor_command=\"echo net.up=1\"\n").unwrap();
        fs::write(user.join("disk.toml"), "domain=\"disk\"\nmonitor_command=\"echo disk.custom=1\"\n").unwrap();

        let all = load_all(&sys, &user);
        assert_eq!(all.len(), 2);
        let disk = all.iter().find(|s| s.domain == "disk").unwrap();
        assert_eq!(disk.source, Some(OpSkillSource::User));
        assert!(disk.monitor_command.contains("disk.custom"));
        assert!(all.iter().any(|s| s.domain == "net" && s.source == Some(OpSkillSource::System)));

        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn malformed_manifest_is_skipped() {
        let sys = tmpdir();
        let user = tmpdir();
        fs::write(sys.join("ok.toml"), "domain=\"d\"\nmonitor_command=\"echo a=1\"\n").unwrap();
        fs::write(sys.join("bad.toml"), "domain = \"\"\n").unwrap();
        assert_eq!(load_all(&sys, &user).len(), 1);
        fs::remove_dir_all(&sys).ok();
        fs::remove_dir_all(&user).ok();
    }

    #[test]
    fn template_is_parseable() {
        assert!(parse(&template("disk")).is_ok());
    }
}
