//! Ephemeral edit history + undo (FEAT-085 / DISC-021).
//!
//! Before `write_file`/`edit_file`/`apply_patch` touches a file, its current
//! on-disk content is snapshotted here (a ring buffer, last 50 edits per
//! file — acceptance criterion 2). `undo` pops the most recent snapshot and
//! reports what to restore the file to (`None` payload = the file didn't
//! exist before, so undoing means deleting it).
//!
//! **In-memory only — lost on daemon restart. Not a git commit or a backup
//! mechanism** (acceptance criterion 4): this is a short-lived safety net
//! for the current session, nothing more.
//!
//! Scope note: the ticket's title mentions "undo/redo" but its acceptance
//! criteria only ask for `undo`/`undo_list` — no `redo` tool is exposed here.
//! Adding one later is a matter of pushing undone records onto a second
//! per-path stack; not built now since nothing asks for it yet.

use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};

/// How many snapshots are kept per file before the oldest is evicted.
const MAX_HISTORY_PER_FILE: usize = 50;

/// One snapshot: a file's content immediately before an edit touched it.
#[derive(Debug, Clone, PartialEq)]
pub struct EditRecord {
    pub path: String,
    pub timestamp: DateTime<Utc>,
    /// Which tool made this edit (`"write_file"`, `"edit_file"`,
    /// `"apply_patch"`), for `undo_list`'s display.
    pub description: String,
    /// The file's content just before the edit. `None` means the file
    /// didn't exist yet — undoing this edit deletes it.
    pub previous_content: Option<String>,
}

/// Per-file ring buffers of edit snapshots.
#[derive(Debug, Default)]
pub struct UndoStore {
    history: HashMap<String, VecDeque<EditRecord>>,
}

impl UndoStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a file's state right before an edit (acceptance criterion 2).
    /// Evicts the oldest snapshot once a file's history exceeds
    /// [`MAX_HISTORY_PER_FILE`].
    pub fn snapshot(&mut self, path: &str, description: &str, previous_content: Option<String>) {
        let entry = self.history.entry(path.to_string()).or_default();
        entry.push_back(EditRecord {
            path: path.to_string(),
            timestamp: Utc::now(),
            description: description.to_string(),
            previous_content,
        });
        while entry.len() > MAX_HISTORY_PER_FILE {
            entry.pop_front();
        }
    }

    /// Pop `path`'s most recent snapshot and return what to restore it to.
    /// `Some(None)` means the file should be deleted (it didn't exist
    /// before the reverted edit); `None` means there's nothing to undo.
    pub fn undo(&mut self, path: &str) -> Option<Option<String>> {
        let entry = self.history.get_mut(path)?;
        let record = entry.pop_back()?;
        if entry.is_empty() {
            self.history.remove(path);
        }
        Some(record.previous_content)
    }

    /// The most recent edits across every file, newest first, bounded by
    /// `limit` (acceptance criterion 3).
    pub fn list_recent(&self, limit: usize) -> Vec<EditRecord> {
        let mut all: Vec<EditRecord> = self.history.values().flat_map(|q| q.iter().cloned()).collect();
        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all.truncate(limit);
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_restores_the_previous_content() {
        let mut store = UndoStore::new();
        store.snapshot("a.txt", "write_file", Some("v1".into()));
        assert_eq!(store.undo("a.txt"), Some(Some("v1".to_string())));
    }

    #[test]
    fn undoing_a_file_that_did_not_exist_signals_delete() {
        let mut store = UndoStore::new();
        store.snapshot("new.txt", "write_file", None);
        assert_eq!(store.undo("new.txt"), Some(None));
    }

    #[test]
    fn undo_with_no_history_returns_none() {
        let mut store = UndoStore::new();
        assert_eq!(store.undo("never-touched.txt"), None);
    }

    #[test]
    fn repeated_undo_walks_back_through_history() {
        let mut store = UndoStore::new();
        store.snapshot("a.txt", "write_file", None);
        store.snapshot("a.txt", "edit_file", Some("v1".into()));
        store.snapshot("a.txt", "edit_file", Some("v2".into()));

        assert_eq!(store.undo("a.txt"), Some(Some("v2".to_string())));
        assert_eq!(store.undo("a.txt"), Some(Some("v1".to_string())));
        assert_eq!(store.undo("a.txt"), Some(None));
        assert_eq!(store.undo("a.txt"), None, "history exhausted");
    }

    #[test]
    fn history_is_capped_and_evicts_the_oldest_snapshot() {
        // acceptance criterion 5: snapshot buffer eviction
        let mut store = UndoStore::new();
        for i in 0..(MAX_HISTORY_PER_FILE + 10) {
            store.snapshot("a.txt", "edit_file", Some(format!("v{i}")));
        }
        // only the most recent MAX_HISTORY_PER_FILE snapshots survive
        let mut restored = Vec::new();
        while let Some(Some(content)) = store.undo("a.txt") {
            restored.push(content);
        }
        assert_eq!(restored.len(), MAX_HISTORY_PER_FILE);
        assert_eq!(restored[0], format!("v{}", MAX_HISTORY_PER_FILE + 9), "most recent first");
    }

    #[test]
    fn list_recent_sorts_newest_first_across_files_and_respects_the_limit() {
        let mut store = UndoStore::new();
        store.snapshot("a.txt", "write_file", None);
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.snapshot("b.txt", "edit_file", Some("x".into()));
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.snapshot("a.txt", "apply_patch", Some("y".into()));

        let all = store.list_recent(10);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].path, "a.txt");
        assert_eq!(all[0].description, "apply_patch");

        let limited = store.list_recent(1);
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].path, "a.txt");
    }

    #[test]
    fn list_recent_on_an_empty_store_is_empty() {
        let store = UndoStore::new();
        assert!(store.list_recent(10).is_empty());
    }
}
