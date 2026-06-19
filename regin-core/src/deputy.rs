//! FEAT-018 (dvalin DISC-037): deputy mode for business continuity. A regin can
//! be the **deputy** of a role: it holds that role's skill package (installed via
//! FEAT-014) plus a **standing continuity brief** (current state, policies, open
//! items — *never* the primary's private Hermes memory), attends the role's
//! meetings as an observer, and **takes over on supervisor-confirmed failover**,
//! handing back when the primary returns.
//!
//! This module is the deputy state machine + brief persistence — unit-tested. The
//! observer attendance is just the bus client subscribed to the role's channel
//! (FEAT-010); failover detection/confirmation is dvalin's (the supervisor).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Whether the deputy is currently covering the role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeputyState {
    /// Following along (brief + observer) but not acting as the role.
    Standby,
    /// Covering the role after a confirmed failover.
    Active,
}

/// The deputy record: which role is covered, the primary, the current state, and
/// the standing continuity brief.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeputyRecord {
    pub role: String,
    pub primary: String,
    pub state: DeputyState,
    #[serde(default)]
    pub brief: String,
}

/// File-backed deputy state. `REGIN_DEPUTY_FILE` overrides the path.
pub struct DeputyStore {
    path: PathBuf,
}

impl DeputyStore {
    pub fn new(path: &Path) -> Self {
        Self { path: path.to_path_buf() }
    }

    pub fn from_env() -> Result<Self> {
        let path = std::env::var("REGIN_DEPUTY_FILE")
            .map(PathBuf::from)
            .or_else(|_| crate::config::data_dir().map(|d| d.join("deputy.json")))?;
        Ok(Self::new(&path))
    }

    pub fn load(&self) -> Result<Option<DeputyRecord>> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => Ok(Some(serde_json::from_str(&s).context("parsing deputy record")?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("reading deputy record"),
        }
    }

    fn save(&self, rec: &DeputyRecord) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(rec)?)?;
        Ok(())
    }

    /// Assign this regin as the deputy of `role` (held by `primary`), in standby.
    pub fn assign(&self, role: &str, primary: &str) -> Result<DeputyRecord> {
        let rec = DeputyRecord { role: role.to_string(), primary: primary.to_string(), state: DeputyState::Standby, brief: String::new() };
        self.save(&rec)?;
        Ok(rec)
    }

    fn current(&self) -> Result<DeputyRecord> {
        self.load()?.ok_or_else(|| anyhow::anyhow!("no deputy assignment (run `regin deputy assign`)"))
    }

    /// Update the standing continuity brief (maintained by the primary).
    pub fn set_brief(&self, brief: &str) -> Result<()> {
        let mut rec = self.current()?;
        rec.brief = brief.to_string();
        self.save(&rec)
    }

    /// Activate the deputy on failover. **Refuses without supervisor
    /// confirmation** (DISC-037: auto-detect, then supervisor-confirm).
    pub fn activate(&self, confirmed_by_supervisor: bool) -> Result<DeputyRecord> {
        if !confirmed_by_supervisor {
            bail!("failover not confirmed by the supervisor — refusing to activate");
        }
        let mut rec = self.current()?;
        rec.state = DeputyState::Active;
        self.save(&rec)?;
        Ok(rec)
    }

    /// Hand back to the primary when it returns.
    pub fn handback(&self) -> Result<DeputyRecord> {
        let mut rec = self.current()?;
        rec.state = DeputyState::Standby;
        self.save(&rec)?;
        Ok(rec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TmpFile(PathBuf);
    impl TmpFile {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("regin-deputy-{}.json", uuid::Uuid::new_v4()));
            TmpFile(p)
        }
    }
    impl Drop for TmpFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn assign_starts_in_standby() {
        let f = TmpFile::new();
        let s = DeputyStore::new(&f.0);
        assert!(s.load().unwrap().is_none(), "no assignment yet");
        let rec = s.assign("cfo", "regin@cave-cfo").unwrap();
        assert_eq!(rec.state, DeputyState::Standby);
        assert_eq!(rec.role, "cfo");
        assert_eq!(s.load().unwrap().unwrap().primary, "regin@cave-cfo");
    }

    #[test]
    fn activate_requires_supervisor_confirmation() {
        let f = TmpFile::new();
        let s = DeputyStore::new(&f.0);
        s.assign("cfo", "regin@cave-cfo").unwrap();
        // unconfirmed → refused, stays standby
        assert!(s.activate(false).is_err());
        assert_eq!(s.load().unwrap().unwrap().state, DeputyState::Standby);
        // confirmed → active
        let rec = s.activate(true).unwrap();
        assert_eq!(rec.state, DeputyState::Active);
        // hand back → standby
        assert_eq!(s.handback().unwrap().state, DeputyState::Standby);
    }

    #[test]
    fn brief_round_trips_and_persists() {
        let f = TmpFile::new();
        let s = DeputyStore::new(&f.0);
        s.assign("cfo", "regin@cave-cfo").unwrap();
        s.set_brief("budget freeze until Q2; open: vendor renewal").unwrap();
        assert!(s.load().unwrap().unwrap().brief.contains("vendor renewal"));
        // activation keeps the brief
        s.activate(true).unwrap();
        assert!(s.load().unwrap().unwrap().brief.contains("budget freeze"));
    }

    #[test]
    fn operations_before_assign_error() {
        let f = TmpFile::new();
        let s = DeputyStore::new(&f.0);
        assert!(s.set_brief("x").is_err());
        assert!(s.activate(true).is_err());
        assert!(s.handback().is_err());
    }
}
