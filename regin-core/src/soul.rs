//! Soul configurator + value catalog (FEAT-030 / DISC-018).
//!
//! The **seed** stage of the value pipeline: a curated, versioned catalog of
//! values drawn broadly across human traditions (not parochial), a
//! **core + per-role overlay** model (a persistent identity-core charter in
//! `identity.db`, plus a swappable per-Persona overlay declared in
//! `persona.toml`), and role → default-values derivation for a sensible
//! starting point.
//!
//! The core charter is written **only** through [`charter_seed`] /
//! [`charter_remove`] here — the privileged human path (`regin soul
//! charter`, FEAT-030 acceptance criterion 4). The general memory verbs and
//! the curator/reflection pipeline refuse to touch `category = "principle"`
//! rows (enforced in [`crate::identity_db::memory_update`],
//! [`crate::identity_db::memory_delete`], and
//! [`crate::identity_db::curator_apply_proposal`]).
//!
//! The Soul (FEAT-029) reads the **grounding**: [`grounding_union`] of the
//! core charter's value ids and the active Persona's `values` overlay.

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use crate::identity_db::{self, PRINCIPLE_CATEGORY};
use crate::types::Memory;

/// One entry in the value catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tradition: String,
}

/// The bundled, versioned value catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct ValueCatalog {
    pub version: String,
    #[serde(rename = "value")]
    pub values: Vec<ValueEntry>,
}

const CATALOG_TOML: &str = include_str!("../assets/values.toml");

static CATALOG: OnceLock<ValueCatalog> = OnceLock::new();

/// The bundled value catalog, parsed once.
pub fn catalog() -> &'static ValueCatalog {
    CATALOG.get_or_init(|| {
        toml::from_str(CATALOG_TOML).expect("assets/values.toml must parse — checked in, compiled in")
    })
}

/// Look up one catalog entry by id.
pub fn find(id: &str) -> Option<&'static ValueEntry> {
    catalog().values.iter().find(|v| v.id == id)
}

/// Built-in role -> default-values map (FEAT-030). Unknown roles fall back to
/// the generic agent-operational virtue set — a sensible, values-neutral
/// starting point pending a human's `regin soul charter --derive` review (the
/// ticket's LLM-assisted suggestion for novel roles is a caller-side
/// enhancement over this fallback, not a change to it — this function stays
/// deterministic and instant).
pub fn role_default_values(role: &str) -> Vec<&'static str> {
    match role {
        "cfo" => vec!["prudence", "integrity", "accountability", "stewardship"],
        "dev-lead" => vec!["diligence", "courage", "honesty", "reason"],
        "operator" => vec!["prudence", "stewardship", "restraint", "transparency"],
        "security" => vec!["caution", "integrity", "accountability", "diligence"],
        "foreman" => vec!["diligence", "accountability", "courtesy", "reason"],
        "auditor" => vec!["honesty", "integrity", "fairness", "transparency"],
        _ => GENERIC_FALLBACK.to_vec(),
    }
}

/// The role-agnostic fallback: the agent-operational virtue set.
const GENERIC_FALLBACK: &[&str] = &["integrity", "caution", "stewardship", "humility"];

/// The deduplicated union of the core charter's value ids and a Persona's
/// overlay — what the Soul reads. Core ids come first, then overlay ids not
/// already in the core, each in their original order.
pub fn grounding_union(core_ids: &[String], persona_overlay: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(core_ids.len() + persona_overlay.len());
    for id in core_ids.iter().chain(persona_overlay.iter()) {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// Seed the identity-core charter with `value_ids` (the privileged write
/// path — only reachable from `regin soul charter`). Idempotent: ids already
/// seeded are skipped. Returns the newly-created rows (already-present ids
/// are not re-returned). Errors on an unknown id — the caller should show
/// the catalog first.
pub fn charter_seed(conn: &Connection, value_ids: &[&str]) -> Result<Vec<Memory>> {
    let existing = charter_core_ids(conn)?;
    let mut created = Vec::new();
    for id in value_ids {
        if existing.iter().any(|e| e == id) {
            continue;
        }
        let entry = find(id).ok_or_else(|| anyhow!("unknown value id {id:?} — see `regin soul values list`"))?;
        created.push(insert_principle(conn, entry)?);
    }
    Ok(created)
}

fn insert_principle(conn: &Connection, entry: &ValueEntry) -> Result<Memory> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let content = charter_content(entry);
    conn.execute(
        "INSERT INTO memories (id, category, content, tier, source, trust_score, pinned, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'long', 'human', 1.0, 1, ?4, ?4)",
        params![&id, PRINCIPLE_CATEGORY, &content, &now],
    )?;
    Ok(Memory {
        id,
        category: PRINCIPLE_CATEGORY.into(),
        content,
        created_at: now.clone(),
        updated_at: now,
        strength: 1,
        last_seen: None,
        source: "human".into(),
    })
}

/// The stored content for a charter row: `"{id}: {name} — {description}"` —
/// the leading `"{id}:"` is how [`charter_core_ids`] recovers the value id
/// (the `memories` table has no dedicated column for it; encoding it in
/// content avoids a schema migration for a feature this narrow).
fn charter_content(entry: &ValueEntry) -> String {
    format!("{}: {} — {}", entry.id, entry.name, entry.description)
}

/// Extract the value ids of the current core charter (pinned, human-sourced,
/// `category = "principle"` memories).
pub fn charter_core_ids(conn: &Connection) -> Result<Vec<String>> {
    let rows = identity_db::memory_list(conn, Some(PRINCIPLE_CATEGORY))?;
    Ok(rows
        .into_iter()
        .filter_map(|m| m.content.split_once(':').map(|(id, _)| id.to_string()))
        .collect())
}

/// Remove a value from the core charter by id (the privileged path — only
/// reachable from `regin soul charter`). Returns whether a row was removed.
pub fn charter_remove(conn: &Connection, value_id: &str) -> Result<bool> {
    let prefix = format!("{value_id}:%");
    let n = conn.execute(
        "DELETE FROM memories WHERE category = ?1 AND content LIKE ?2",
        params![PRINCIPLE_CATEGORY, &prefix],
    )?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        identity_db::init_identity_schema(&c).unwrap();
        c
    }

    #[test]
    fn catalog_loads_and_is_versioned() {
        let cat = catalog();
        assert_eq!(cat.version, "1");
        assert!(cat.values.len() > 20, "broad catalog, not a token gesture");
        assert!(find("integrity").is_some());
        assert!(find("prudence").is_some());
    }

    #[test]
    fn every_entry_has_a_description_and_tradition() {
        for v in &catalog().values {
            assert!(!v.description.is_empty(), "{} missing a description", v.id);
            assert!(!v.tradition.is_empty(), "{} missing a tradition tag", v.id);
        }
    }

    #[test]
    fn catalog_ids_are_unique() {
        let mut ids: Vec<&str> = catalog().values.iter().map(|v| v.id.as_str()).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate value id in the catalog");
    }

    #[test]
    fn role_default_values_known_role() {
        let v = role_default_values("cfo");
        assert!(v.contains(&"prudence"));
        assert!(v.contains(&"accountability"));
        for id in &v {
            assert!(find(id).is_some(), "role map references unknown id {id}");
        }
    }

    #[test]
    fn role_default_values_unknown_role_falls_back_to_generic() {
        assert_eq!(role_default_values("time-traveler"), GENERIC_FALLBACK);
    }

    #[test]
    fn grounding_union_dedupes_core_and_overlay() {
        let core = vec!["integrity".to_string(), "prudence".to_string()];
        let overlay = vec!["prudence".to_string(), "courage".to_string()];
        assert_eq!(grounding_union(&core, &overlay), vec!["integrity", "prudence", "courage"]);
    }

    #[test]
    fn charter_seed_writes_pinned_human_principle_rows() {
        let c = conn();
        let created = charter_seed(&c, &["integrity", "prudence"]).unwrap();
        assert_eq!(created.len(), 2);
        for m in &created {
            assert_eq!(m.category, "principle");
            assert_eq!(m.source, "human");
        }
        let pinned: i64 = c.query_row(
            "SELECT COUNT(*) FROM memories WHERE category = 'principle' AND pinned = 1 AND source = 'human'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(pinned, 2);
    }

    #[test]
    fn charter_seed_is_idempotent() {
        let c = conn();
        charter_seed(&c, &["integrity"]).unwrap();
        let second = charter_seed(&c, &["integrity", "prudence"]).unwrap();
        assert_eq!(second.len(), 1, "integrity already seeded, only prudence is new");
        assert_eq!(charter_core_ids(&c).unwrap().len(), 2);
    }

    #[test]
    fn charter_seed_rejects_unknown_id() {
        let c = conn();
        assert!(charter_seed(&c, &["made_up_value"]).is_err());
    }

    #[test]
    fn charter_core_ids_recovers_seeded_ids() {
        let c = conn();
        charter_seed(&c, &["integrity", "prudence", "courage"]).unwrap();
        let mut ids = charter_core_ids(&c).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["courage", "integrity", "prudence"]);
    }

    #[test]
    fn charter_remove_deletes_by_value_id() {
        let c = conn();
        charter_seed(&c, &["integrity", "prudence"]).unwrap();
        assert!(charter_remove(&c, "integrity").unwrap());
        assert_eq!(charter_core_ids(&c).unwrap(), vec!["prudence".to_string()]);
        assert!(!charter_remove(&c, "integrity").unwrap(), "already gone");
    }

    #[test]
    fn charter_derive_then_confirm_matches_role_default_and_is_queryable_as_the_union() {
        // Simulates `regin soul charter --derive` (propose) then confirmation (write).
        let c = conn();
        let proposal = role_default_values("operator");
        charter_seed(&c, &proposal).unwrap();
        let core = charter_core_ids(&c).unwrap();
        let overlay: Vec<String> = vec!["restraint".to_string()]; // already in core, dedup check
        let grounding = grounding_union(&core, &overlay);
        for id in &proposal {
            assert!(grounding.contains(&id.to_string()));
        }
        assert_eq!(grounding.len(), core.len(), "overlay id already in core, no growth");
    }
}
