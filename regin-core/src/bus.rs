//! FEAT-010 (DISC-004): regin's in-cave messaging-bus client. dvalin owns the
//! bus; execd bridges the cave boundary (dvalin FEAT-124): it drops inbound mail
//! into a cave **inbox** file and drains a cave **outbox** file. This module is
//! the regin side — read the inbox, append to the outbox — so regin speaks the
//! bus without a network socket of its own.
//!
//! Identity is the agent's `role@cave` address. Two message modes mirror dvalin:
//! `unstructured` (free-text inform/request) and `structured` (typed JSON body,
//! e.g. a work handover).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

pub const KIND_UNSTRUCTURED: &str = "unstructured";
pub const KIND_STRUCTURED: &str = "structured";

const DEFAULT_INBOX: &str = "/var/lib/regin/inbox.jsonl";
const DEFAULT_OUTBOX: &str = "/var/lib/regin/outbox.jsonl";

/// One bus message. Mirrors dvalin's `Message` shape on the wire (the fields
/// execd writes into the inbox / reads from the outbox).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusMessage {
    #[serde(default)]
    pub id: i64,
    pub sender: String,
    pub recipient: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub body: String,
    #[serde(default)]
    pub ref_id: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
}

fn default_kind() -> String {
    KIND_UNSTRUCTURED.to_string()
}

/// The bus client: our address + the inbox/outbox file paths + a cursor file
/// that records how many inbox lines we've already consumed (at-least-once: a
/// crash re-reads from the cursor).
pub struct BusClient {
    address: String,
    inbox: PathBuf,
    outbox: PathBuf,
    cursor: PathBuf,
}

impl BusClient {
    /// Build from the environment: `REGIN_ADDRESS` (required for a real cave),
    /// `REGIN_INBOX` / `REGIN_OUTBOX` overriding the cave defaults.
    pub fn from_env() -> Result<Self> {
        let address = std::env::var("REGIN_ADDRESS")
            .context("REGIN_ADDRESS not set (the agent's role@cave bus identity)")?;
        let inbox = std::env::var("REGIN_INBOX").unwrap_or_else(|_| DEFAULT_INBOX.into());
        let outbox = std::env::var("REGIN_OUTBOX").unwrap_or_else(|_| DEFAULT_OUTBOX.into());
        Ok(Self::new(&address, Path::new(&inbox), Path::new(&outbox)))
    }

    pub fn new(address: &str, inbox: &Path, outbox: &Path) -> Self {
        let cursor = inbox.with_extension("cursor");
        Self {
            address: address.to_string(),
            inbox: inbox.to_path_buf(),
            outbox: outbox.to_path_buf(),
            cursor,
        }
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// Append a message to the outbox for execd to relay onto the bus. The sender
    /// is always stamped as our own address (execd re-stamps authoritatively, but
    /// we set it honestly here too). Returns the serialized line.
    pub fn send(&self, to: &str, kind: &str, body: &str, ref_id: Option<&str>) -> Result<()> {
        let msg = BusMessage {
            id: 0,
            sender: self.address.clone(),
            recipient: to.to_string(),
            kind: kind.to_string(),
            body: body.to_string(),
            ref_id: ref_id.map(String::from),
            channel: None,
        };
        if let Some(parent) = self.outbox.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let line = serde_json::to_string(&msg)?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.outbox)
            .with_context(|| format!("opening outbox {}", self.outbox.display()))?;
        writeln!(f, "{line}")?;
        Ok(())
    }

    /// Convenience: send a free-text message.
    pub fn inform(&self, to: &str, body: &str) -> Result<()> {
        self.send(to, KIND_UNSTRUCTURED, body, None)
    }

    /// Read inbox messages we have not yet consumed. With `mark`, advance the
    /// cursor past them (so the next call returns only newer mail); without, it's
    /// a non-destructive peek.
    pub fn inbox(&self, mark: bool) -> Result<Vec<BusMessage>> {
        let consumed = self.read_cursor();
        let lines: Vec<String> = match std::fs::File::open(&self.inbox) {
            Ok(f) => std::io::BufReader::new(f)
                .lines()
                .collect::<std::result::Result<Vec<_>, _>>()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e).context("reading inbox"),
        };
        let total = lines.len();
        let mut out = Vec::new();
        for line in lines.into_iter().skip(consumed) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<BusMessage>(line) {
                Ok(m) => out.push(m),
                Err(e) => tracing::warn!("skipping malformed inbox line: {e}"),
            }
        }
        if mark {
            self.write_cursor(total)?;
        }
        Ok(out)
    }

    fn read_cursor(&self) -> usize {
        std::fs::read_to_string(&self.cursor)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    fn write_cursor(&self, n: usize) -> Result<()> {
        if let Some(parent) = self.cursor.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&self.cursor, n.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TmpDir(PathBuf);
    impl TmpDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("regin-bus-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            TmpDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn client(dir: &Path) -> BusClient {
        BusClient::new("regin@cave-a", &dir.join("inbox.jsonl"), &dir.join("outbox.jsonl"))
    }

    #[test]
    fn send_appends_a_stamped_outbox_line() {
        let tmp = TmpDir::new();
        let c = client(tmp.path());
        c.send("ceo@hq", KIND_STRUCTURED, "{\"k\":1}", Some("T-1")).unwrap();
        c.inform("cto@hq", "status green").unwrap();
        let lines: Vec<BusMessage> = std::fs::read_to_string(tmp.path().join("outbox.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].sender, "regin@cave-a");
        assert_eq!(lines[0].recipient, "ceo@hq");
        assert_eq!(lines[0].kind, KIND_STRUCTURED);
        assert_eq!(lines[0].ref_id.as_deref(), Some("T-1"));
        assert_eq!(lines[1].kind, KIND_UNSTRUCTURED);
        assert_eq!(lines[1].body, "status green");
    }

    #[test]
    fn inbox_reads_unconsumed_and_advances_cursor() {
        let tmp = TmpDir::new();
        let c = client(tmp.path());
        // simulate execd dropping two messages
        let drop = |m: &BusMessage| {
            let mut f = std::fs::OpenOptions::new().create(true).append(true)
                .open(tmp.path().join("inbox.jsonl")).unwrap();
            writeln!(f, "{}", serde_json::to_string(m).unwrap()).unwrap();
        };
        drop(&BusMessage { id: 1, sender: "ceo@hq".into(), recipient: "regin@cave-a".into(), kind: KIND_UNSTRUCTURED.into(), body: "one".into(), ref_id: None, channel: None });
        drop(&BusMessage { id: 2, sender: "ceo@hq".into(), recipient: "regin@cave-a".into(), kind: KIND_UNSTRUCTURED.into(), body: "two".into(), ref_id: None, channel: None });

        // peek does not advance
        assert_eq!(c.inbox(false).unwrap().len(), 2);
        assert_eq!(c.inbox(false).unwrap().len(), 2, "peek is non-destructive");
        // consume advances the cursor
        let first = c.inbox(true).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].body, "one");
        assert!(c.inbox(true).unwrap().is_empty(), "all consumed");
        // a new drop is seen
        drop(&BusMessage { id: 3, sender: "ceo@hq".into(), recipient: "regin@cave-a".into(), kind: KIND_UNSTRUCTURED.into(), body: "three".into(), ref_id: None, channel: None });
        let next = c.inbox(true).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].body, "three");
    }

    #[test]
    fn inbox_missing_file_is_empty_not_error() {
        let tmp = TmpDir::new();
        assert!(client(tmp.path()).inbox(true).unwrap().is_empty());
    }

    #[test]
    fn inbox_skips_malformed_lines() {
        let tmp = TmpDir::new();
        std::fs::write(tmp.path().join("inbox.jsonl"), "not json\n{\"sender\":\"a@b\",\"recipient\":\"regin@cave-a\",\"body\":\"ok\"}\n").unwrap();
        let got = client(tmp.path()).inbox(true).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "ok");
        assert_eq!(got[0].kind, KIND_UNSTRUCTURED, "defaulted kind");
    }
}
