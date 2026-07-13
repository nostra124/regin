//! Goal model + store (FEAT-061 / DISC-019): "achieve a state by a date."
//!
//! A goal is a dated intent — description + target + deadline — with
//! **success criteria derived at planning time**: measurable/structural
//! preferred (a to-be-state-shaped assertion checked against a supplied
//! observation, reusing `desired::Assertion`'s key/op/value shape and
//! `evaluate::satisfies`), LLM-judged only where measurement is too fuzzy
//! (the measurable-preferred / LLM-fallback rule, DISC-019). Lifecycle:
//! `proposed -> active -> achieved | failed | abandoned`.
//!
//! `priority`/`source`/`rag` reuse `objective::IntentSource`/`objective::Rag`
//! — the same shared intent vocabulary an objective carries (DISC-019: both
//! objectives and goals are "intents").
//!
//! FEAT-061's own scope is the store + done-detection given an externally
//! supplied observation map and LLM judge — deriving criteria from a goal's
//! free-text description is the planner's job (FEAT-063); wiring
//! `evaluate_goal` into the daemon's live loop is later planning-control-loop
//! work (FEAT-066).

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::desired::{AssertOp, AssertValue};
use crate::evaluate;
use crate::objective::{IntentSource, Rag};

/// A goal's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    Proposed,
    Active,
    Achieved,
    Failed,
    Abandoned,
}

impl GoalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalStatus::Proposed => "proposed",
            GoalStatus::Active => "active",
            GoalStatus::Achieved => "achieved",
            GoalStatus::Failed => "failed",
            GoalStatus::Abandoned => "abandoned",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "proposed" => GoalStatus::Proposed,
            "active" => GoalStatus::Active,
            "achieved" => GoalStatus::Achieved,
            "failed" => GoalStatus::Failed,
            "abandoned" => GoalStatus::Abandoned,
            other => bail!("unknown goal status: {other:?}"),
        })
    }

    fn is_terminal(&self) -> bool {
        matches!(self, GoalStatus::Achieved | GoalStatus::Failed | GoalStatus::Abandoned)
    }
}

/// A single derived success criterion — measurable-preferred (checked
/// deterministically against a supplied observation) or LLM-judged when
/// measurement is too fuzzy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SuccessCriterion {
    /// Structural/measurable: holds when `key`'s observed value satisfies
    /// `op`/`value` (same evaluation as an instantaneous to-be-state
    /// assertion — reuses `evaluate::satisfies`).
    Measurable {
        key: String,
        op: AssertOp,
        value: AssertValue,
        description: Option<String>,
    },
    /// Too fuzzy to measure structurally — judged by an LLM.
    Judged { description: String },
}

/// Judges whether a fuzzy [`SuccessCriterion::Judged`] currently holds.
/// Injectable so tests never need a real LLM call (acceptance criterion 2).
#[async_trait]
pub trait GoalJudge: Send + Sync {
    async fn holds(&self, goal_description: &str, criterion_description: &str) -> Result<bool>;
}

/// A fixed-answer judge for tests: every fuzzy criterion resolves to the
/// same verdict.
pub struct FixedGoalJudge(pub bool);

#[async_trait]
impl GoalJudge for FixedGoalJudge {
    async fn holds(&self, _goal_description: &str, _criterion_description: &str) -> Result<bool> {
        Ok(self.0)
    }
}

/// A dated goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    /// LLM-authored free text describing the goal.
    pub description: String,
    /// The target end-state, in prose (structured criteria are derived
    /// separately — `criteria`).
    pub target: String,
    pub deadline: String,
    pub criteria: Vec<SuccessCriterion>,
    pub priority: i64,
    pub source: String,
    pub rag: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

const GOAL_COLS: &str =
    "id, description, target, deadline, criteria_json, priority, source, rag, status, created_at, updated_at";

fn row_to_goal(row: &rusqlite::Row) -> rusqlite::Result<Goal> {
    let criteria_json: String = row.get(4)?;
    let criteria: Vec<SuccessCriterion> = serde_json::from_str(&criteria_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Goal {
        id: row.get(0)?,
        description: row.get(1)?,
        target: row.get(2)?,
        deadline: row.get(3)?,
        criteria,
        priority: row.get(5)?,
        source: row.get(6)?,
        rag: row.get(7)?,
        status: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

/// Create a goal, starting `proposed` and `green` (DISC-019 lifecycle).
/// `source` is validated via [`IntentSource::parse`] so a garbage value is
/// refused at creation.
pub fn goal_create(
    conn: &Connection,
    description: &str,
    target: &str,
    deadline: &str,
    criteria: Vec<SuccessCriterion>,
    priority: i64,
    source: &str,
) -> Result<Goal> {
    IntentSource::parse(source)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let criteria_json = serde_json::to_string(&criteria)?;
    conn.execute(
        "INSERT INTO goals \
            (id, description, target, deadline, criteria_json, priority, source, rag, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'green', 'proposed', ?8, ?8)",
        params![&id, description, target, deadline, &criteria_json, priority, source, &now],
    )?;
    goal_get(conn, &id)?.context("goal vanished after insert")
}

pub fn goal_get(conn: &Connection, id: &str) -> Result<Option<Goal>> {
    let sql = format!("SELECT {GOAL_COLS} FROM goals WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_goal)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn goal_list(conn: &Connection, status: Option<&str>) -> Result<Vec<Goal>> {
    let (sql, p): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
        Some(s) => (
            format!("SELECT {GOAL_COLS} FROM goals WHERE status = ?1 ORDER BY priority ASC, deadline ASC"),
            vec![Box::new(s.to_string())],
        ),
        None => (
            format!("SELECT {GOAL_COLS} FROM goals ORDER BY priority ASC, deadline ASC"),
            vec![],
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(p.iter()), row_to_goal)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn goal_set_status(conn: &Connection, id: &str, status: GoalStatus, rag: Rag) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let n = conn.execute(
        "UPDATE goals SET status = ?1, rag = ?2, updated_at = ?3 WHERE id = ?4",
        params![status.as_str(), rag.as_str(), &now, id],
    )?;
    if n == 0 {
        bail!("no goal {id}");
    }
    Ok(())
}

/// `proposed -> active` (DISC-019 lifecycle). Errors on any other starting
/// state — activation is a deliberate transition, not idempotent.
pub fn goal_activate(conn: &Connection, id: &str) -> Result<Goal> {
    let goal = goal_get(conn, id)?.context(format!("no goal {id}"))?;
    if GoalStatus::parse(&goal.status)? != GoalStatus::Proposed {
        bail!("goal {id} is {} — only a proposed goal can be activated", goal.status);
    }
    goal_set_status(conn, id, GoalStatus::Active, Rag::Green)?;
    goal_get(conn, id)?.context("goal vanished after activation")
}

/// Any non-terminal state -> `abandoned` (a human/regin decision, never
/// automatic — [`evaluate_goal`] never produces this outcome itself).
pub fn goal_abandon(conn: &Connection, id: &str) -> Result<Goal> {
    let goal = goal_get(conn, id)?.context(format!("no goal {id}"))?;
    if GoalStatus::parse(&goal.status)?.is_terminal() {
        bail!("goal {id} is already {} — nothing to abandon", goal.status);
    }
    goal_set_status(conn, id, GoalStatus::Abandoned, Rag::Red)?;
    goal_get(conn, id)?.context("goal vanished after abandon")
}

/// The result of evaluating an active goal's criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOutcome {
    /// Not every criterion holds yet, and the deadline hasn't passed.
    StillActive,
    /// Every criterion holds — the goal transitions to `achieved`.
    Achieved,
    /// The deadline has passed with criteria unmet — the goal transitions
    /// to `failed`.
    Failed,
    /// The goal isn't `active` (proposed or already terminal) — nothing to
    /// evaluate; the goal is left untouched.
    NotActive,
}

/// Whether every criterion currently holds: measurable criteria are checked
/// against `observed` via `evaluate::satisfies` (a missing key counts as
/// *not* holding — unconfirmed success is not success, the opposite
/// convention from `evaluate::evaluate`'s deviation detection, where a
/// missing key is silently skipped); fuzzy criteria are asked of `judge`.
async fn all_criteria_hold(
    goal: &Goal,
    observed: &BTreeMap<String, AssertValue>,
    judge: &dyn GoalJudge,
) -> Result<bool> {
    for c in &goal.criteria {
        let holds = match c {
            SuccessCriterion::Measurable { key, op, value, .. } => match observed.get(key) {
                Some(obs) => evaluate::satisfies(obs, *op, value),
                None => false,
            },
            SuccessCriterion::Judged { description } => judge.holds(&goal.description, description).await?,
        };
        if !holds {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Evaluate an **active** goal's done-detection: achieved when every
/// criterion holds; auto-failed once `now` is at/past the deadline with
/// criteria still unmet (acceptance criterion 3 — `now` is caller-supplied
/// so tests use a fake clock, never wall-clock time). Transitions the
/// goal's stored status/RAG accordingly; a still-active goal is left
/// untouched (re-evaluated on the next pass, once the planning control
/// loop exists to drive that, FEAT-066).
pub async fn evaluate_goal(
    conn: &Connection,
    goal_id: &str,
    observed: &BTreeMap<String, AssertValue>,
    judge: &dyn GoalJudge,
    now: DateTime<Utc>,
) -> Result<GoalOutcome> {
    let goal = goal_get(conn, goal_id)?.context(format!("no goal {goal_id}"))?;
    if GoalStatus::parse(&goal.status)? != GoalStatus::Active {
        return Ok(GoalOutcome::NotActive);
    }

    if all_criteria_hold(&goal, observed, judge).await? {
        goal_set_status(conn, goal_id, GoalStatus::Achieved, Rag::Green)?;
        return Ok(GoalOutcome::Achieved);
    }

    let deadline = DateTime::parse_from_rfc3339(&goal.deadline)
        .with_context(|| format!("goal {goal_id} has an unparseable deadline: {:?}", goal.deadline))?
        .with_timezone(&Utc);
    if now >= deadline {
        goal_set_status(conn, goal_id, GoalStatus::Failed, Rag::Red)?;
        return Ok(GoalOutcome::Failed);
    }

    Ok(GoalOutcome::StillActive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn future_deadline(days: i64) -> String {
        (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339()
    }

    fn a_measurable_criterion() -> SuccessCriterion {
        SuccessCriterion::Measurable {
            key: "disk.root.use_percent".into(),
            op: AssertOp::Lt,
            value: AssertValue::Num(80.0),
            description: Some("root stays under 80%".into()),
        }
    }

    #[test]
    fn create_get_list_round_trip_and_starts_proposed_green() {
        let c = conn();
        let goal = goal_create(
            &c, "shrink disk usage", "root under 80%", &future_deadline(30),
            vec![a_measurable_criterion()], 2, "human",
        ).unwrap();
        assert_eq!(goal.status, "proposed");
        assert_eq!(goal.rag, "green");
        assert_eq!(goal.criteria.len(), 1);

        let fetched = goal_get(&c, &goal.id).unwrap().unwrap();
        assert_eq!(fetched, goal);
        assert_eq!(goal_list(&c, None).unwrap().len(), 1);
        assert!(goal_get(&c, "no-such-id").unwrap().is_none());
    }

    #[test]
    fn create_rejects_an_unknown_source() {
        let c = conn();
        assert!(goal_create(&c, "d", "t", &future_deadline(1), vec![], 1, "the-vibes").is_err());
        assert_eq!(goal_list(&c, None).unwrap().len(), 0);
    }

    #[test]
    fn proposed_goals_can_be_abandoned_directly() {
        let c = conn();
        let goal = goal_create(&c, "d", "t", &future_deadline(1), vec![], 1, "human").unwrap();
        let abandoned = goal_abandon(&c, &goal.id).unwrap();
        assert_eq!(abandoned.status, "abandoned");
        assert_eq!(abandoned.rag, "red");
    }

    #[test]
    fn activate_requires_proposed_and_is_not_idempotent() {
        let c = conn();
        let goal = goal_create(&c, "d", "t", &future_deadline(1), vec![], 1, "human").unwrap();
        let active = goal_activate(&c, &goal.id).unwrap();
        assert_eq!(active.status, "active");
        assert!(goal_activate(&c, &goal.id).is_err(), "already active, not proposed");
    }

    #[test]
    fn terminal_goals_cannot_be_abandoned() {
        let c = conn();
        let goal = goal_create(&c, "d", "t", &future_deadline(1), vec![], 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();
        // force it to a terminal state directly for the test
        goal_set_status(&c, &goal.id, GoalStatus::Achieved, Rag::Green).unwrap();
        assert!(goal_abandon(&c, &goal.id).is_err());
    }

    #[tokio::test]
    async fn evaluate_goal_is_a_noop_on_a_non_active_goal() {
        let c = conn();
        let goal = goal_create(&c, "d", "t", &future_deadline(1), vec![a_measurable_criterion()], 1, "human").unwrap();
        let observed = BTreeMap::new();
        let outcome = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(true), chrono::Utc::now()).await.unwrap();
        assert_eq!(outcome, GoalOutcome::NotActive, "still proposed, never activated");
        assert_eq!(goal_get(&c, &goal.id).unwrap().unwrap().status, "proposed");
    }

    #[tokio::test]
    async fn evaluate_goal_achieves_when_the_measurable_criterion_holds() {
        let c = conn();
        let goal = goal_create(&c, "d", "t", &future_deadline(30), vec![a_measurable_criterion()], 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();

        let mut observed = BTreeMap::new();
        observed.insert("disk.root.use_percent".to_string(), AssertValue::Num(50.0));
        let outcome = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(true), chrono::Utc::now()).await.unwrap();

        assert_eq!(outcome, GoalOutcome::Achieved);
        let fetched = goal_get(&c, &goal.id).unwrap().unwrap();
        assert_eq!(fetched.status, "achieved");
        assert_eq!(fetched.rag, "green");
    }

    #[tokio::test]
    async fn evaluate_goal_stays_active_when_unmet_before_the_deadline() {
        let c = conn();
        let goal = goal_create(&c, "d", "t", &future_deadline(30), vec![a_measurable_criterion()], 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();

        let mut observed = BTreeMap::new();
        observed.insert("disk.root.use_percent".to_string(), AssertValue::Num(95.0)); // breaches "< 80"
        let outcome = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(true), chrono::Utc::now()).await.unwrap();

        assert_eq!(outcome, GoalOutcome::StillActive);
        assert_eq!(goal_get(&c, &goal.id).unwrap().unwrap().status, "active");
    }

    #[tokio::test]
    async fn evaluate_goal_a_missing_observation_never_counts_as_achieved() {
        let c = conn();
        let goal = goal_create(&c, "d", "t", &future_deadline(30), vec![a_measurable_criterion()], 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();

        let observed = BTreeMap::new(); // no data at all for the key
        let outcome = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(true), chrono::Utc::now()).await.unwrap();
        assert_eq!(outcome, GoalOutcome::StillActive, "unconfirmed is not achieved");
    }

    #[tokio::test]
    async fn evaluate_goal_auto_fails_past_the_deadline_with_a_fake_clock() {
        // acceptance criterion 3
        let c = conn();
        let past_deadline = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let goal = goal_create(&c, "d", "t", &past_deadline, vec![a_measurable_criterion()], 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();

        let observed = BTreeMap::new(); // criterion unmet
        let outcome = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(true), chrono::Utc::now()).await.unwrap();

        assert_eq!(outcome, GoalOutcome::Failed);
        let fetched = goal_get(&c, &goal.id).unwrap().unwrap();
        assert_eq!(fetched.status, "failed");
        assert_eq!(fetched.rag, "red");
    }

    #[tokio::test]
    async fn evaluate_goal_prefers_achievement_over_failure_right_at_the_deadline() {
        let c = conn();
        let past_deadline = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        let goal = goal_create(&c, "d", "t", &past_deadline, vec![a_measurable_criterion()], 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();

        let mut observed = BTreeMap::new();
        observed.insert("disk.root.use_percent".to_string(), AssertValue::Num(10.0)); // holds
        let outcome = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(true), chrono::Utc::now()).await.unwrap();
        assert_eq!(outcome, GoalOutcome::Achieved, "criteria holding wins even if the deadline has technically passed");
    }

    #[tokio::test]
    async fn a_fuzzy_criterion_uses_the_injected_judge() {
        // acceptance criterion 2
        let c = conn();
        let criterion = SuccessCriterion::Judged { description: "the on-call team reports things feel snappier".into() };
        let goal = goal_create(&c, "d", "t", &future_deadline(30), vec![criterion], 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();

        let observed = BTreeMap::new();
        let not_yet = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(false), chrono::Utc::now()).await.unwrap();
        assert_eq!(not_yet, GoalOutcome::StillActive);

        let achieved = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(true), chrono::Utc::now()).await.unwrap();
        assert_eq!(achieved, GoalOutcome::Achieved);
    }

    #[tokio::test]
    async fn every_criterion_must_hold_mixed_measurable_and_judged() {
        let c = conn();
        let criteria = vec![
            a_measurable_criterion(),
            SuccessCriterion::Judged { description: "looks good".into() },
        ];
        let goal = goal_create(&c, "d", "t", &future_deadline(30), criteria, 1, "human").unwrap();
        goal_activate(&c, &goal.id).unwrap();

        let mut observed = BTreeMap::new();
        observed.insert("disk.root.use_percent".to_string(), AssertValue::Num(10.0)); // measurable holds

        // fuzzy criterion doesn't hold -> overall not achieved despite the measurable one holding
        let outcome = evaluate_goal(&c, &goal.id, &observed, &FixedGoalJudge(false), chrono::Utc::now()).await.unwrap();
        assert_eq!(outcome, GoalOutcome::StillActive);
    }

    #[test]
    fn list_filters_by_status() {
        let c = conn();
        let a = goal_create(&c, "a", "t", &future_deadline(1), vec![], 1, "human").unwrap();
        let _b = goal_create(&c, "b", "t", &future_deadline(1), vec![], 2, "human").unwrap();
        goal_activate(&c, &a.id).unwrap();

        assert_eq!(goal_list(&c, Some("active")).unwrap().len(), 1);
        assert_eq!(goal_list(&c, Some("proposed")).unwrap().len(), 1);
        assert_eq!(goal_list(&c, None).unwrap().len(), 2);
    }
}
