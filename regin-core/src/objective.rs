//! Objective model (FEAT-060 / DISC-019): "maintain a state over time."
//!
//! Generalizes the DISC-008 to-be-state (`desired.rs`) so a target can be a
//! **KPI aggregate over a rolling time window** (e.g. "LLM spend stays under
//! $50/30d"), not only an instantaneous observed signal. An objective breach
//! is evaluated and raised through the exact same observed-vs-target
//! machinery an instantaneous to-be-state deviation already uses
//! (`evaluate::satisfies` + `evaluate::raise_for_deviations`, FEAT-034/037)
//! — no parallel evaluator.
//!
//! Distinct from `kpi::Objective` (the CSI cost-vs-reliability constrained
//! function) — this is regin's own goal-directed "maintain X" intent
//! (DISC-019). `IntentSource` and `Rag` are shared vocabulary FEAT-061
//! (goals) reuses.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::desired::{AssertOp, AssertValue};
use crate::evaluate::{self, Deviation};
use crate::kpi;

/// Who authored/owns an intent — who escalations route back to (FEAT-069).
/// Shared vocabulary: FEAT-061 (goals) carries the same field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentSource {
    Human,
    Dvalin,
    Regin,
}

impl IntentSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            IntentSource::Human => "human",
            IntentSource::Dvalin => "dvalin",
            IntentSource::Regin => "regin",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "human" => IntentSource::Human,
            "dvalin" => IntentSource::Dvalin,
            "regin" => IntentSource::Regin,
            other => bail!("unknown intent source: {other:?}"),
        })
    }
}

/// Coarse RAG health. This module only ever computes green (assertion
/// holds) or red (breached) — FEAT-064's scheduler later computes the
/// nuanced amber ("off-track but mitigated, not yet endangered").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rag {
    Green,
    Amber,
    Red,
}

impl Rag {
    pub fn as_str(&self) -> &'static str {
        match self {
            Rag::Green => "green",
            Rag::Amber => "amber",
            Rag::Red => "red",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "green" => Rag::Green,
            "amber" => Rag::Amber,
            "red" => Rag::Red,
            other => bail!("unknown RAG value: {other:?}"),
        })
    }
}

/// How a windowed KPI value is computed from the raw event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KpiAggregate {
    Sum,
    Count,
}

impl KpiAggregate {
    pub fn as_str(&self) -> &'static str {
        match self {
            KpiAggregate::Sum => "sum",
            KpiAggregate::Count => "count",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_lowercase().as_str() {
            "sum" => KpiAggregate::Sum,
            "count" => KpiAggregate::Count,
            other => bail!("unknown KPI aggregate: {other:?}"),
        })
    }
}

/// A standing objective: "maintain `metric`'s `aggregate` over the trailing
/// `window_days`, `op` `value`" (e.g. `cost.llm_usd` summed over 30 days
/// stays under $50). String fields (`aggregate`/`op`/`source`/`rag`) mirror
/// this crate's existing ITIL-record convention (validated at the
/// create/update boundary, stored loosely) rather than round-tripping typed
/// enums through SQLite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Objective {
    pub id: String,
    pub title: String,
    pub description: String,
    pub metric: String,
    pub aggregate: String,
    pub window_days: i64,
    pub op: String,
    pub value: AssertValue,
    /// Lower is more urgent; arbitration semantics land with FEAT-062.
    pub priority: i64,
    pub source: String,
    pub rag: String,
    pub created_at: String,
    pub updated_at: String,
}

const OBJECTIVE_COLS: &str = "id, title, description, metric, aggregate, window_days, op, \
     value_num, value_text, priority, source, rag, created_at, updated_at";

fn row_to_objective(row: &rusqlite::Row) -> rusqlite::Result<Objective> {
    let value_num: Option<f64> = row.get(7)?;
    let value_text: Option<String> = row.get(8)?;
    let value = match value_num {
        Some(n) => AssertValue::Num(n),
        None => AssertValue::Text(value_text.unwrap_or_default()),
    };
    Ok(Objective {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        metric: row.get(3)?,
        aggregate: row.get(4)?,
        window_days: row.get(5)?,
        op: row.get(6)?,
        value,
        priority: row.get(9)?,
        source: row.get(10)?,
        rag: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

/// Create an objective. `op`/`aggregate`/`source` are validated (must parse
/// via [`AssertOp::parse`]/[`KpiAggregate::parse`]/[`IntentSource::parse`])
/// so a garbage value is refused at creation, not discovered at evaluation
/// time. Starts at RAG `green` (untested until the first [`check_objectives`]
/// pass).
#[allow(clippy::too_many_arguments)]
pub fn objective_create(
    conn: &Connection,
    title: &str,
    description: &str,
    metric: &str,
    aggregate: &str,
    window_days: i64,
    op: &str,
    value: &AssertValue,
    priority: i64,
    source: &str,
) -> Result<Objective> {
    AssertOp::parse(op)?;
    KpiAggregate::parse(aggregate)?;
    IntentSource::parse(source)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let (value_num, value_text): (Option<f64>, Option<&str>) = match value {
        AssertValue::Num(n) => (Some(*n), None),
        AssertValue::Text(t) => (None, Some(t.as_str())),
    };
    conn.execute(
        "INSERT INTO objectives \
            (id, title, description, metric, aggregate, window_days, op, value_num, value_text, priority, source, rag, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'green', ?12, ?12)",
        params![&id, title, description, metric, aggregate, window_days, op, value_num, value_text, priority, source, &now],
    )?;
    objective_get(conn, &id)?.context("objective vanished after insert")
}

pub fn objective_get(conn: &Connection, id: &str) -> Result<Option<Objective>> {
    let sql = format!("SELECT {OBJECTIVE_COLS} FROM objectives WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![id], row_to_objective)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn objective_list(conn: &Connection) -> Result<Vec<Objective>> {
    let sql = format!("SELECT {OBJECTIVE_COLS} FROM objectives ORDER BY priority ASC, created_at ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_objective)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn objective_set_rag(conn: &Connection, id: &str, rag: Rag) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE objectives SET rag = ?1, updated_at = ?2 WHERE id = ?3",
        params![rag.as_str(), &now, id],
    )?;
    Ok(())
}

/// Compute the objective's KPI aggregate over its trailing window as an
/// `AssertValue` observation, ready to compare via [`evaluate::satisfies`] —
/// the exact machinery instantaneous to-be-state assertions already use.
pub fn observe(conn: &Connection, objective: &Objective) -> Result<AssertValue> {
    let since = (chrono::Utc::now() - chrono::Duration::days(objective.window_days)).to_rfc3339();
    let value = match KpiAggregate::parse(&objective.aggregate)? {
        KpiAggregate::Sum => kpi::kpi_sum(conn, &objective.metric, &since)?,
        KpiAggregate::Count => kpi::kpi_count(conn, &objective.metric, &since)? as f64,
    };
    Ok(AssertValue::Num(value))
}

/// Evaluate one objective against the KPI store. `None` means it currently
/// holds; `Some(deviation)` describes the breach, reusing `evaluate::Deviation`
/// — the exact type an instantaneous to-be-state breach produces (acceptance
/// criterion 1).
pub fn evaluate_objective(conn: &Connection, objective: &Objective) -> Result<Option<Deviation>> {
    let observed = observe(conn, objective)?;
    let op = AssertOp::parse(&objective.op)?;
    if evaluate::satisfies(&observed, op, &objective.value) {
        return Ok(None);
    }
    Ok(Some(Deviation {
        key: format!("objective:{}:{}:{}d", objective.metric, objective.aggregate, objective.window_days),
        op,
        target: objective.value.clone(),
        observed,
        detail: Some(objective.title.clone()),
    }))
}

/// Evaluate every stored objective, persist its resulting coarse RAG, and —
/// for a breach — raise/dedupe an incident through the existing
/// observed-vs-target loop (`evaluate::raise_for_deviations`, keyed per
/// objective so incidents don't cross-contaminate). Acceptance criterion 3:
/// a breach flows through the existing loop, not a new path. Returns each
/// objective's id paired with the incident id raised/touched for it, if any.
pub fn check_objectives(conn: &Connection, severity: &str) -> Result<Vec<(String, Option<String>)>> {
    let objectives = objective_list(conn)?;
    let mut results = Vec::with_capacity(objectives.len());
    for obj in &objectives {
        let deviation = evaluate_objective(conn, obj)?;
        objective_set_rag(conn, &obj.id, if deviation.is_some() { Rag::Red } else { Rag::Green })?;
        let incident_id = match &deviation {
            Some(d) => evaluate::raise_for_deviations(
                conn,
                &format!("objective:{}", obj.id),
                std::slice::from_ref(d),
                severity,
            )?,
            None => None,
        };
        results.push((obj.id.clone(), incident_id));
    }
    Ok(results)
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

    fn a_cost_objective(conn: &Connection, window_days: i64, ceiling: f64) -> Objective {
        objective_create(
            conn,
            "hold LLM spend down",
            "keep cost under control",
            kpi::M_COST_LLM,
            "sum",
            window_days,
            "le",
            &AssertValue::Num(ceiling),
            1,
            "human",
        ).unwrap()
    }

    #[test]
    fn create_get_list_round_trip_priority_and_source() {
        let c = conn();
        let obj = a_cost_objective(&c, 30, 50.0);
        assert_eq!(obj.rag, "green", "untested objectives start green");
        assert_eq!(obj.priority, 1);
        assert_eq!(obj.source, "human");

        let fetched = objective_get(&c, &obj.id).unwrap().unwrap();
        assert_eq!(fetched, obj);

        let listed = objective_list(&c).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, obj.id);

        assert!(objective_get(&c, "no-such-id").unwrap().is_none());
    }

    #[test]
    fn create_rejects_an_unknown_op_aggregate_or_source() {
        let c = conn();
        assert!(objective_create(&c, "t", "d", "m", "sum", 30, "wat", &AssertValue::Num(1.0), 1, "human").is_err());
        assert!(objective_create(&c, "t", "d", "m", "median", 30, "le", &AssertValue::Num(1.0), 1, "human").is_err());
        assert!(objective_create(&c, "t", "d", "m", "sum", 30, "le", &AssertValue::Num(1.0), 1, "the-vibes").is_err());
        assert_eq!(objective_list(&c).unwrap().len(), 0, "no partial writes from a rejected create");
    }

    #[test]
    fn observe_sums_only_events_inside_the_window() {
        let c = conn();
        let obj = a_cost_objective(&c, 7, 100.0);
        kpi::kpi_record(&c, kpi::M_COST_LLM, 10.0, None).unwrap();
        kpi::kpi_record(&c, kpi::M_COST_LLM, 5.0, None).unwrap();
        // an event well outside the 7-day window
        let old = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        c.execute(
            "INSERT INTO kpi_events (id, recorded_at, metric, value, meta) VALUES ('old', ?1, ?2, 1000.0, NULL)",
            rusqlite::params![old, kpi::M_COST_LLM],
        ).unwrap();

        assert_eq!(observe(&c, &obj).unwrap(), AssertValue::Num(15.0));
    }

    #[test]
    fn observe_counts_events_for_a_count_aggregate() {
        let c = conn();
        let obj = objective_create(
            &c, "few incidents", "d", kpi::M_REMEDIATION_ESCALATED, "count", 30, "le",
            &AssertValue::Num(3.0), 1, "regin",
        ).unwrap();
        kpi::kpi_record(&c, kpi::M_REMEDIATION_ESCALATED, 1.0, None).unwrap();
        kpi::kpi_record(&c, kpi::M_REMEDIATION_ESCALATED, 1.0, None).unwrap();
        assert_eq!(observe(&c, &obj).unwrap(), AssertValue::Num(2.0));
    }

    #[test]
    fn evaluate_objective_holds_and_breaches() {
        let c = conn();
        let obj = a_cost_objective(&c, 30, 20.0);

        // no events yet -> sum is 0, well under the ceiling -> holds
        assert!(evaluate_objective(&c, &obj).unwrap().is_none());

        kpi::kpi_record(&c, kpi::M_COST_LLM, 25.0, None).unwrap();
        let dev = evaluate_objective(&c, &obj).unwrap().unwrap();
        assert_eq!(dev.observed, AssertValue::Num(25.0));
        assert_eq!(dev.target, AssertValue::Num(20.0));
        assert_eq!(dev.op, AssertOp::Le);
    }

    #[test]
    fn check_objectives_raises_and_dedupes_an_incident_and_sets_rag() {
        let c = conn();
        let obj = a_cost_objective(&c, 30, 20.0);
        kpi::kpi_record(&c, kpi::M_COST_LLM, 25.0, None).unwrap();

        let first = check_objectives(&c, "high").unwrap();
        assert_eq!(first.len(), 1);
        let incident_id = first[0].1.clone().expect("breach raises an incident");
        assert_eq!(db::incident_list(&c, None).unwrap().len(), 1);
        assert_eq!(objective_get(&c, &obj.id).unwrap().unwrap().rag, "red");

        // re-checking while still breached dedupes to the same incident
        let second = check_objectives(&c, "high").unwrap();
        assert_eq!(second[0].1, Some(incident_id));
        assert_eq!(db::incident_list(&c, None).unwrap().len(), 1, "no duplicate incident");
    }

    #[test]
    fn check_objectives_reports_no_incident_and_green_rag_when_it_holds() {
        let c = conn();
        let obj = a_cost_objective(&c, 30, 20.0);
        kpi::kpi_record(&c, kpi::M_COST_LLM, 5.0, None).unwrap();

        let result = check_objectives(&c, "high").unwrap();
        assert_eq!(result, vec![(obj.id.clone(), None)]);
        assert_eq!(db::incident_list(&c, None).unwrap().len(), 0);
        assert_eq!(objective_get(&c, &obj.id).unwrap().unwrap().rag, "green");
    }

    #[test]
    fn list_orders_by_priority_then_creation() {
        let c = conn();
        let low = objective_create(&c, "low", "d", "m", "sum", 30, "le", &AssertValue::Num(1.0), 5, "human").unwrap();
        let high = objective_create(&c, "high", "d", "m", "sum", 30, "le", &AssertValue::Num(1.0), 1, "human").unwrap();
        let listed = objective_list(&c).unwrap();
        assert_eq!(listed[0].id, high.id);
        assert_eq!(listed[1].id, low.id);
    }
}
