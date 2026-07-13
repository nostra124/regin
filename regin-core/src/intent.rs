//! Intent dependency & conflict graph (FEAT-062 / DISC-019).
//!
//! A relation store over the two intent kinds — goals (FEAT-061) and
//! objectives (FEAT-060) — with two relation kinds:
//! - **`supports`**: achieving the `from` intent advances the `to` intent.
//! - **`conflicts_with`**: pursuing both `from` and `to` at once pulls apart.
//!
//! Conflict arbitration is priority-based (lower `priority` number wins, the
//! same "lower is more urgent" convention `objective::Objective::priority`
//! and `goal::Goal::priority` already use) and records a **mitigation** for
//! the deferred intent, deduped the same way `evaluate::raise_for_deviations`
//! dedupes incidents — re-arbitrating an already-mitigated pair doesn't
//! create a duplicate.
//!
//! `supports` propagation doesn't reach into `goal`/`objective` internals
//! (no "progress" field on either): a supporting relation is `credited` when
//! its `from` intent is reported achieved, and a supported intent's progress
//! is simply "how many of my supporters are credited" — queryable without
//! coupling this module's shape to either store's schema.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::goal;
use crate::objective;

/// Which store an intent id refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentKind {
    Goal,
    Objective,
}

impl IntentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            IntentKind::Goal => "goal",
            IntentKind::Objective => "objective",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "goal" => IntentKind::Goal,
            "objective" => IntentKind::Objective,
            other => bail!("unknown intent kind: {other:?}"),
        })
    }
}

/// How one intent relates to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationKind {
    Supports,
    ConflictsWith,
}

impl RelationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationKind::Supports => "supports",
            RelationKind::ConflictsWith => "conflicts_with",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "supports" => RelationKind::Supports,
            "conflicts_with" => RelationKind::ConflictsWith,
            other => bail!("unknown relation kind: {other:?}"),
        })
    }
}

/// A directed relation between two intents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub id: String,
    pub from_kind: String,
    pub from_id: String,
    pub to_kind: String,
    pub to_id: String,
    pub relation: String,
    /// Set when `from`'s achievement has been credited toward `to`'s
    /// progress (`supports` relations only — see [`record_achievement`]).
    pub credited_at: Option<String>,
    pub created_at: String,
}

const RELATION_COLS: &str = "id, from_kind, from_id, to_kind, to_id, relation, credited_at, created_at";

fn row_to_relation(row: &rusqlite::Row) -> rusqlite::Result<Relation> {
    Ok(Relation {
        id: row.get(0)?,
        from_kind: row.get(1)?,
        from_id: row.get(2)?,
        to_kind: row.get(3)?,
        to_id: row.get(4)?,
        relation: row.get(5)?,
        credited_at: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn intent_exists(conn: &Connection, kind: IntentKind, id: &str) -> Result<bool> {
    match kind {
        IntentKind::Goal => Ok(goal::goal_get(conn, id)?.is_some()),
        IntentKind::Objective => Ok(objective::objective_get(conn, id)?.is_some()),
    }
}

fn intent_priority(conn: &Connection, kind: IntentKind, id: &str) -> Result<i64> {
    match kind {
        IntentKind::Goal => Ok(goal::goal_get(conn, id)?.context(format!("no goal {id}"))?.priority),
        IntentKind::Objective => {
            Ok(objective::objective_get(conn, id)?.context(format!("no objective {id}"))?.priority)
        }
    }
}

/// Whether an intent is currently being actively pursued. A goal is active
/// only in its `active` lifecycle status; an objective has no lifecycle of
/// its own — it stands as long as it exists (FEAT-060 never retires one).
fn intent_is_active(conn: &Connection, kind: IntentKind, id: &str) -> Result<bool> {
    match kind {
        IntentKind::Goal => Ok(goal::goal_get(conn, id)?.is_some_and(|g| g.status == "active")),
        IntentKind::Objective => Ok(objective::objective_get(conn, id)?.is_some()),
    }
}

/// Create a `supports`/`conflicts_with` relation between two intents.
/// `from_kind`/`to_kind` must be `"goal"` or `"objective"` and must refer to
/// an existing record — a garbage or dangling reference is refused at
/// creation, not discovered later (this crate's established validate-at-the-
/// boundary convention, e.g. `objective_create`, `goal_create`).
pub fn relation_create(
    conn: &Connection,
    from_kind: &str,
    from_id: &str,
    to_kind: &str,
    to_id: &str,
    relation: &str,
) -> Result<Relation> {
    let fk = IntentKind::parse(from_kind)?;
    let tk = IntentKind::parse(to_kind)?;
    RelationKind::parse(relation)?;
    if !intent_exists(conn, fk, from_id)? {
        bail!("no such {from_kind} {from_id}");
    }
    if !intent_exists(conn, tk, to_id)? {
        bail!("no such {to_kind} {to_id}");
    }
    if fk == tk && from_id == to_id {
        bail!("an intent cannot relate to itself");
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO intent_relations \
            (id, from_kind, from_id, to_kind, to_id, relation, credited_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
        params![&id, from_kind, from_id, to_kind, to_id, relation, &now],
    )?;
    relation_get(conn, &id)?.context("relation vanished after insert")
}

pub fn relation_get(conn: &Connection, id: &str) -> Result<Option<Relation>> {
    let sql = format!("SELECT {RELATION_COLS} FROM intent_relations WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_relation)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// Relations where the given intent is the `from` side (queryable direction
/// 1 of acceptance criterion 1).
pub fn relations_from(conn: &Connection, kind: &str, id: &str) -> Result<Vec<Relation>> {
    let sql = format!("SELECT {RELATION_COLS} FROM intent_relations WHERE from_kind = ?1 AND from_id = ?2 ORDER BY created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![kind, id], row_to_relation)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Relations where the given intent is the `to` side (queryable direction 2
/// of acceptance criterion 1).
pub fn relations_to(conn: &Connection, kind: &str, id: &str) -> Result<Vec<Relation>> {
    let sql = format!("SELECT {RELATION_COLS} FROM intent_relations WHERE to_kind = ?1 AND to_id = ?2 ORDER BY created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![kind, id], row_to_relation)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// A resolved conflict: `winner` keeps priority, `deferred` is the one a
/// mitigation was recorded for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictArbitration {
    pub relation_id: String,
    pub winner_kind: String,
    pub winner_id: String,
    pub deferred_kind: String,
    pub deferred_id: String,
    pub mitigation_id: String,
}

fn find_existing_mitigation(
    conn: &Connection,
    deferred_kind: &str,
    deferred_id: &str,
    winner_kind: &str,
    winner_id: &str,
) -> Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM intent_mitigations \
         WHERE deferred_kind = ?1 AND deferred_id = ?2 AND winner_kind = ?3 AND winner_id = ?4",
    )?;
    let mut rows = stmt.query_map(params![deferred_kind, deferred_id, winner_kind, winner_id], |r| r.get::<_, String>(0))?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

/// Detect every currently-active `conflicts_with` pair (both endpoints
/// active — acceptance criterion 2) and arbitrate by priority: the
/// lower-`priority`-number intent wins, the other is deferred and gets a
/// mitigation recorded (deduped: re-arbitrating an already-mitigated pair
/// returns the same mitigation id rather than creating a duplicate, mirroring
/// `evaluate::raise_for_deviations`'s dedupe convention). Ties break on id
/// ordering, deterministically.
pub fn arbitrate_conflicts(conn: &Connection, note: &str) -> Result<Vec<ConflictArbitration>> {
    let sql = format!("SELECT {RELATION_COLS} FROM intent_relations WHERE relation = 'conflicts_with' ORDER BY created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let relations = stmt
        .query_map([], row_to_relation)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut results = Vec::new();
    for rel in relations {
        let fk = IntentKind::parse(&rel.from_kind)?;
        let tk = IntentKind::parse(&rel.to_kind)?;
        if !intent_is_active(conn, fk, &rel.from_id)? || !intent_is_active(conn, tk, &rel.to_id)? {
            continue;
        }

        let from_priority = intent_priority(conn, fk, &rel.from_id)?;
        let to_priority = intent_priority(conn, tk, &rel.to_id)?;
        let from_wins = from_priority < to_priority || (from_priority == to_priority && rel.from_id < rel.to_id);
        let (winner_kind, winner_id, deferred_kind, deferred_id) = if from_wins {
            (rel.from_kind.clone(), rel.from_id.clone(), rel.to_kind.clone(), rel.to_id.clone())
        } else {
            (rel.to_kind.clone(), rel.to_id.clone(), rel.from_kind.clone(), rel.from_id.clone())
        };

        let mitigation_id = match find_existing_mitigation(conn, &deferred_kind, &deferred_id, &winner_kind, &winner_id)? {
            Some(existing) => existing,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO intent_mitigations \
                        (id, winner_kind, winner_id, deferred_kind, deferred_id, note, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![&id, &winner_kind, &winner_id, &deferred_kind, &deferred_id, note, &now],
                )?;
                id
            }
        };

        results.push(ConflictArbitration {
            relation_id: rel.id,
            winner_kind,
            winner_id,
            deferred_kind,
            deferred_id,
            mitigation_id,
        });
    }
    Ok(results)
}

/// Mitigations recorded for an intent as the deferred side of an arbitrated
/// conflict.
pub fn mitigations_for(conn: &Connection, kind: &str, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM intent_mitigations WHERE deferred_kind = ?1 AND deferred_id = ?2 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![kind, id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Report that `(kind, id)` has been achieved: every `supports` relation
/// where it's the `from` side gets credited, advancing each supported
/// intent's progress. Returns the `(kind, id)` pairs of intents touched
/// (acceptance criterion 3).
pub fn record_achievement(conn: &Connection, kind: &str, id: &str) -> Result<Vec<(String, String)>> {
    IntentKind::parse(kind)?;
    let mut stmt = conn.prepare(
        "SELECT id, to_kind, to_id FROM intent_relations \
         WHERE from_kind = ?1 AND from_id = ?2 AND relation = 'supports' AND credited_at IS NULL",
    )?;
    let rows = stmt
        .query_map(params![kind, id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut touched = Vec::with_capacity(rows.len());
    for (relation_id, to_kind, to_id) in rows {
        conn.execute(
            "UPDATE intent_relations SET credited_at = ?1 WHERE id = ?2",
            params![&now, &relation_id],
        )?;
        touched.push((to_kind, to_id));
    }
    Ok(touched)
}

/// How many of an intent's `supports` relations have been credited — a
/// coarse progress signal that doesn't require a `progress` field on `Goal`
/// or `Objective` themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportProgress {
    pub supporters: usize,
    pub achieved_supporters: usize,
}

impl SupportProgress {
    pub fn fraction(&self) -> f64 {
        if self.supporters == 0 {
            0.0
        } else {
            self.achieved_supporters as f64 / self.supporters as f64
        }
    }
}

pub fn progress_for(conn: &Connection, kind: &str, id: &str) -> Result<SupportProgress> {
    IntentKind::parse(kind)?;
    let mut stmt = conn.prepare(
        "SELECT credited_at FROM intent_relations WHERE to_kind = ?1 AND to_id = ?2 AND relation = 'supports'",
    )?;
    let rows = stmt
        .query_map(params![kind, id], |r| r.get::<_, Option<String>>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let supporters = rows.len();
    let achieved_supporters = rows.iter().filter(|c| c.is_some()).count();
    Ok(SupportProgress { supporters, achieved_supporters })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::desired::AssertValue;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn a_goal(conn: &Connection, priority: i64) -> goal::Goal {
        let deadline = (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339();
        goal::goal_create(conn, "d", "t", &deadline, vec![], priority, "human").unwrap()
    }

    fn an_objective(conn: &Connection, priority: i64) -> objective::Objective {
        objective::objective_create(
            conn, "t", "d", "m", "sum", 30, "le", &AssertValue::Num(10.0), priority, "human",
        ).unwrap()
    }

    #[test]
    fn relation_create_rejects_unknown_kinds_relations_and_dangling_ids() {
        let c = conn();
        let a = a_goal(&c, 1);
        let b = a_goal(&c, 2);
        assert!(relation_create(&c, "goal", &a.id, "goal", &b.id, "orbits").is_err());
        assert!(relation_create(&c, "planet", &a.id, "goal", &b.id, "supports").is_err());
        assert!(relation_create(&c, "goal", "no-such-id", "goal", &b.id, "supports").is_err());
        assert!(relation_create(&c, "goal", &a.id, "goal", &a.id, "supports").is_err(), "no self-relations");
    }

    #[test]
    fn relations_persist_and_are_queryable_both_directions() {
        // acceptance criterion 1
        let c = conn();
        let a = a_goal(&c, 1);
        let b = an_objective(&c, 1);
        let rel = relation_create(&c, "goal", &a.id, "objective", &b.id, "supports").unwrap();

        let from_a = relations_from(&c, "goal", &a.id).unwrap();
        assert_eq!(from_a.len(), 1);
        assert_eq!(from_a[0].id, rel.id);

        let to_b = relations_to(&c, "objective", &b.id).unwrap();
        assert_eq!(to_b.len(), 1);
        assert_eq!(to_b[0].id, rel.id);

        assert!(relations_to(&c, "goal", &a.id).unwrap().is_empty());
        assert!(relations_from(&c, "objective", &b.id).unwrap().is_empty());
    }

    #[test]
    fn arbitrate_conflicts_ignores_inactive_intents() {
        let c = conn();
        // goals start "proposed", not "active" -> not eligible for arbitration
        let a = a_goal(&c, 1);
        let b = a_goal(&c, 2);
        relation_create(&c, "goal", &a.id, "goal", &b.id, "conflicts_with").unwrap();
        assert!(arbitrate_conflicts(&c, "test").unwrap().is_empty());
    }

    #[test]
    fn arbitrate_conflicts_selects_by_priority_and_records_a_mitigation() {
        // acceptance criterion 2
        let c = conn();
        let urgent = a_goal(&c, 1); // lower number = higher priority
        let less_urgent = a_goal(&c, 5);
        goal::goal_activate(&c, &urgent.id).unwrap();
        goal::goal_activate(&c, &less_urgent.id).unwrap();
        relation_create(&c, "goal", &less_urgent.id, "goal", &urgent.id, "conflicts_with").unwrap();

        let arbitrations = arbitrate_conflicts(&c, "both want the same window").unwrap();
        assert_eq!(arbitrations.len(), 1);
        let a = &arbitrations[0];
        assert_eq!(a.winner_id, urgent.id);
        assert_eq!(a.deferred_id, less_urgent.id);
        assert_eq!(mitigations_for(&c, "goal", &less_urgent.id).unwrap(), vec![a.mitigation_id.clone()]);
        assert!(mitigations_for(&c, "goal", &urgent.id).unwrap().is_empty(), "the winner isn't deferred");
    }

    #[test]
    fn arbitrate_conflicts_dedupes_the_mitigation_on_repeat_calls() {
        let c = conn();
        let urgent = a_goal(&c, 1);
        let less_urgent = a_goal(&c, 5);
        goal::goal_activate(&c, &urgent.id).unwrap();
        goal::goal_activate(&c, &less_urgent.id).unwrap();
        relation_create(&c, "goal", &urgent.id, "goal", &less_urgent.id, "conflicts_with").unwrap();

        let first = arbitrate_conflicts(&c, "note").unwrap();
        let second = arbitrate_conflicts(&c, "note").unwrap();
        assert_eq!(first[0].mitigation_id, second[0].mitigation_id);
        assert_eq!(mitigations_for(&c, "goal", &less_urgent.id).unwrap().len(), 1, "no duplicate mitigation");
    }

    #[test]
    fn arbitrate_conflicts_breaks_ties_deterministically_by_id() {
        let c = conn();
        let a = a_goal(&c, 3);
        let b = a_goal(&c, 3); // same priority
        goal::goal_activate(&c, &a.id).unwrap();
        goal::goal_activate(&c, &b.id).unwrap();
        relation_create(&c, "goal", &a.id, "goal", &b.id, "conflicts_with").unwrap();

        let expected_winner = std::cmp::min(&a.id, &b.id);
        let arbitrations = arbitrate_conflicts(&c, "note").unwrap();
        assert_eq!(&arbitrations[0].winner_id, expected_winner);
    }

    #[test]
    fn achieving_a_supporter_advances_the_supported_intents_progress() {
        // acceptance criterion 3
        let c = conn();
        let supporter = a_goal(&c, 1);
        let supported = a_goal(&c, 1);
        relation_create(&c, "goal", &supporter.id, "goal", &supported.id, "supports").unwrap();

        let before = progress_for(&c, "goal", &supported.id).unwrap();
        assert_eq!(before, SupportProgress { supporters: 1, achieved_supporters: 0 });
        assert_eq!(before.fraction(), 0.0);

        let touched = record_achievement(&c, "goal", &supporter.id).unwrap();
        assert_eq!(touched, vec![("goal".to_string(), supported.id.clone())]);

        let after = progress_for(&c, "goal", &supported.id).unwrap();
        assert_eq!(after, SupportProgress { supporters: 1, achieved_supporters: 1 });
        assert_eq!(after.fraction(), 1.0);
    }

    #[test]
    fn progress_averages_over_multiple_supporters() {
        let c = conn();
        let s1 = a_goal(&c, 1);
        let s2 = a_goal(&c, 1);
        let supported = an_objective(&c, 1);
        relation_create(&c, "goal", &s1.id, "objective", &supported.id, "supports").unwrap();
        relation_create(&c, "goal", &s2.id, "objective", &supported.id, "supports").unwrap();

        record_achievement(&c, "goal", &s1.id).unwrap();

        let progress = progress_for(&c, "objective", &supported.id).unwrap();
        assert_eq!(progress, SupportProgress { supporters: 2, achieved_supporters: 1 });
        assert_eq!(progress.fraction(), 0.5);
    }

    #[test]
    fn record_achievement_is_a_noop_when_nothing_is_supported() {
        let c = conn();
        let lone = a_goal(&c, 1);
        assert!(record_achievement(&c, "goal", &lone.id).unwrap().is_empty());
    }

    #[test]
    fn conflicts_with_between_a_goal_and_an_objective_is_detected() {
        let c = conn();
        let g = a_goal(&c, 1);
        goal::goal_activate(&c, &g.id).unwrap();
        let o = an_objective(&c, 5); // objectives are always "active"
        relation_create(&c, "goal", &g.id, "objective", &o.id, "conflicts_with").unwrap();

        let arbitrations = arbitrate_conflicts(&c, "cross-kind conflict").unwrap();
        assert_eq!(arbitrations.len(), 1);
        assert_eq!(arbitrations[0].winner_id, g.id, "goal has the lower (more urgent) priority number");
        assert_eq!(arbitrations[0].deferred_id, o.id);
    }
}
