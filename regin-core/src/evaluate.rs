//! Observed-vs-target evaluation (FEAT-034 / DISC-008).
//!
//! Monitoring is redefined from "a run errored ⇒ incident" (the FEAT-004 framing)
//! to **observed vs target**: observed signals are checked against the domain's
//! to-be-state assertions (FEAT-033), and only a genuine *deviation* from intent
//! raises an incident. A run error becomes just one input to judgement, not an
//! automatic incident.
//!
//! The structured-assertion check here is deterministic and fully tested; the
//! [`DeviationJudge`] trait is the seam where the LLM tier (FEAT-049) judges
//! whether an unstructured/novel observation is worth raising.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::desired::{AssertOp, AssertValue, DesiredState};

/// A single observed breach of a target assertion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deviation {
    pub key: String,
    pub op: AssertOp,
    pub target: AssertValue,
    pub observed: AssertValue,
    pub detail: Option<String>,
}

impl std::fmt::Display for Deviation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is {} (target {} {})",
            self.key,
            self.observed,
            self.op.as_str(),
            self.target
        )
    }
}

const EPS: f64 = f64::EPSILON;

/// Whether an observed value satisfies an assertion. A type mismatch, or an
/// ordering operator applied to text, counts as *not satisfied* (a deviation).
pub fn satisfies(observed: &AssertValue, op: AssertOp, target: &AssertValue) -> bool {
    match (observed, target) {
        (AssertValue::Num(o), AssertValue::Num(t)) => match op {
            AssertOp::Lt => o < t,
            AssertOp::Le => o <= t,
            AssertOp::Gt => o > t,
            AssertOp::Ge => o >= t,
            AssertOp::Eq => (o - t).abs() <= EPS,
            AssertOp::Ne => (o - t).abs() > EPS,
        },
        (AssertValue::Text(o), AssertValue::Text(t)) => match op {
            AssertOp::Eq => o == t,
            AssertOp::Ne => o != t,
            // ordering operators are undefined on text -> treat as a deviation
            _ => false,
        },
        // numeric-vs-text mismatch -> deviation
        _ => false,
    }
}

/// Evaluate observed signals against a domain's target. Keys without an observed
/// value are skipped (no data to judge). Returns every assertion the observation
/// breaches.
pub fn evaluate(ds: &DesiredState, observed: &BTreeMap<String, AssertValue>) -> Vec<Deviation> {
    let mut deviations = Vec::new();
    for a in &ds.assertions {
        if let Some(obs) = observed.get(&a.key)
            && !satisfies(obs, a.op, &a.value)
        {
            deviations.push(Deviation {
                key: a.key.clone(),
                op: a.op,
                target: a.value.clone(),
                observed: obs.clone(),
                detail: a.description.clone(),
            });
        }
    }
    deviations
}

/// Judges whether a deviation is worth an incident. The deterministic default
/// treats every structured-assertion breach as worth raising; the LLM tier
/// (FEAT-049) supplies a richer judge for unstructured signals.
pub trait DeviationJudge {
    fn worth_incident(&self, deviation: &Deviation) -> bool;
}

/// Default judge: a structured-assertion breach is always worth an incident.
pub struct StructuredJudge;

impl DeviationJudge for StructuredJudge {
    fn worth_incident(&self, _deviation: &Deviation) -> bool {
        true
    }
}

/// Raise an incident for a domain's deviations, deduped against an existing active
/// incident for the domain (one open incident per domain "shape", like the
/// monitor). Returns the incident id when one is opened/updated, else `None`.
pub fn raise_for_deviations(
    conn: &Connection,
    domain: &str,
    deviations: &[Deviation],
    severity: &str,
) -> Result<Option<String>> {
    if deviations.is_empty() {
        return Ok(None);
    }
    if let Some(existing) = db::incident_active_for_skill(conn, domain)? {
        db::incident_touch(conn, &existing.id)?;
        return Ok(Some(existing.id));
    }
    let summary = deviations
        .iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let inc = db::incident_open(
        conn,
        &format!("{domain} deviates from target"),
        &summary,
        severity,
        "monitor",
        Some(domain),
    )?;
    db::episode_record(
        conn,
        "incident",
        Some(&inc.id),
        &format!("observed-vs-target deviation for `{domain}`"),
        Some(&summary),
    )?;
    Ok(Some(inc.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desired::{parse_desired_state, DesiredSource};
    use std::path::PathBuf;

    fn num(n: f64) -> AssertValue {
        AssertValue::Num(n)
    }
    fn txt(s: &str) -> AssertValue {
        AssertValue::Text(s.into())
    }

    #[test]
    fn satisfies_numeric_and_text() {
        assert!(satisfies(&num(80.0), AssertOp::Lt, &num(90.0)));
        assert!(!satisfies(&num(95.0), AssertOp::Lt, &num(90.0)));
        assert!(satisfies(&num(90.0), AssertOp::Le, &num(90.0)));
        assert!(satisfies(&num(5.0), AssertOp::Eq, &num(5.0)));
        assert!(satisfies(&num(5.0), AssertOp::Ne, &num(6.0)));
        assert!(satisfies(&txt("rw"), AssertOp::Eq, &txt("rw")));
        assert!(satisfies(&txt("rw"), AssertOp::Ne, &txt("ro")));
        // type mismatch and text-ordering are deviations
        assert!(!satisfies(&txt("rw"), AssertOp::Lt, &txt("ro")));
        assert!(!satisfies(&num(1.0), AssertOp::Eq, &txt("1")));
    }

    fn disk_ds() -> DesiredState {
        let md = "# disk\n\n```assertions\n[[assert]]\nkey=\"disk.root.use_percent\"\nop=\"lt\"\nvalue=90\n[[assert]]\nkey=\"disk.fs.mode\"\nop=\"eq\"\nvalue=\"rw\"\n```\n";
        parse_desired_state("disk", md, DesiredSource::System, PathBuf::new()).unwrap()
    }

    #[test]
    fn evaluate_flags_only_breaches() {
        let ds = disk_ds();
        // all within target
        let mut obs = BTreeMap::new();
        obs.insert("disk.root.use_percent".to_string(), num(70.0));
        obs.insert("disk.fs.mode".to_string(), txt("rw"));
        assert!(evaluate(&ds, &obs).is_empty());

        // breach the numeric target
        obs.insert("disk.root.use_percent".to_string(), num(95.0));
        let devs = evaluate(&ds, &obs);
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].key, "disk.root.use_percent");

        // breach both
        obs.insert("disk.fs.mode".to_string(), txt("ro"));
        assert_eq!(evaluate(&ds, &obs).len(), 2);
    }

    #[test]
    fn missing_observation_is_skipped() {
        let ds = disk_ds();
        let obs = BTreeMap::new(); // no data
        assert!(evaluate(&ds, &obs).is_empty(), "no data -> nothing judged");
    }

    #[test]
    fn structured_judge_always_worth() {
        let d = Deviation { key: "k".into(), op: AssertOp::Lt, target: num(1.0), observed: num(2.0), detail: None };
        assert!(StructuredJudge.worth_incident(&d));
    }

    #[test]
    fn raise_opens_and_dedups_incident() {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        let ds = disk_ds();
        let mut obs = BTreeMap::new();
        obs.insert("disk.root.use_percent".to_string(), num(95.0));
        let devs = evaluate(&ds, &obs);

        // no deviations -> no incident
        assert!(raise_for_deviations(&c, "disk", &[], "high").unwrap().is_none());

        // first deviation opens an incident
        let id1 = raise_for_deviations(&c, "disk", &devs, "high").unwrap().unwrap();
        assert_eq!(db::incident_list(&c, None).unwrap().len(), 1);
        // second while still active dedups to the same incident
        let id2 = raise_for_deviations(&c, "disk", &devs, "high").unwrap().unwrap();
        assert_eq!(id1, id2);
        assert_eq!(db::incident_list(&c, None).unwrap().len(), 1);
    }
}
