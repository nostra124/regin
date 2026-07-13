//! Event bus + triggers (FEAT-067 / DISC-019).
//!
//! A small internal typed event bus — the bridge between the reactive plane
//! (0.5.0: incidents, deviations) and the proactive one (0.7.0: objectives,
//! goals, plans). Publishers across both planes fire well-known kinds
//! (`incident.created`, `objective.breached`, `deviation.detected`,
//! `goal.created`, `schedule.tick`, `task.completed`, `task.failed`, ...);
//! **triggers** bind a kind (+ optional condition) to an action.
//!
//! **External ingestion reuses this crate's existing structured-bus-body
//! convention** (a JSON `"kind"` tag inside the body — the same idiom
//! `escalation.rs`/`chair.rs`/`foreman.rs`/`planning.rs` already use for
//! their own structured payloads): [`event_from_bus_message`] maps an
//! inbound `bus::BusMessage` straight into an [`Event`] using the body's own
//! `"kind"` field, rather than inventing a second envelope format.
//!
//! **Publishing an event always satisfies a matching `Task::depends_on_events`
//! entry**, independent of whatever triggers happen to be registered —
//! dependency satisfaction is a property of the event having fired at all,
//! not of a bound action running successfully (acceptance criterion 1).
//!
//! **Fail-safe**: a trigger's action is `Result`-fallible; an error is
//! logged and recorded in the `PublishReport`, but never stops the bus from
//! running the remaining triggers or from recording the event into the
//! ledger (acceptance criterion 3).

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use crate::bus::{self, BusMessage};
use crate::task_network::Task;

// --- well-known event kinds ------------------------------------------------

pub const INCIDENT_CREATED: &str = "incident.created";
pub const OBJECTIVE_BREACHED: &str = "objective.breached";
pub const DEVIATION_DETECTED: &str = "deviation.detected";
pub const GOAL_CREATED: &str = "goal.created";
pub const SCHEDULE_TICK: &str = "schedule.tick";
pub const TASK_COMPLETED: &str = "task.completed";
pub const TASK_FAILED: &str = "task.failed";

/// One event on the bus: a well-known (or externally-sourced) kind plus a
/// free-form JSON payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub kind: String,
    pub payload: serde_json::Value,
}

impl Event {
    pub fn new(kind: impl Into<String>, payload: serde_json::Value) -> Self {
        Self { kind: kind.into(), payload }
    }
}

/// What a trigger does once its event fires: instantiate a task/plan, or
/// anything else a caller wires up. Deliberately abstract — this module
/// doesn't couple to `task_network`/`task_executor`'s concrete shapes, so
/// callers supply their own action (mirrors `task_executor::ActionRunner`'s
/// injectable-trait pattern).
#[async_trait]
pub trait TriggerAction: Send + Sync {
    async fn fire(&self, event: &Event) -> Result<()>;
}

/// A predicate over an event's payload, gating whether a [`Trigger`] fires.
type Condition = Arc<dyn Fn(&serde_json::Value) -> bool + Send + Sync>;

/// Binds an event kind (+ optional condition over the payload) to an
/// action.
pub struct Trigger {
    pub id: String,
    pub event_kind: String,
    condition: Option<Condition>,
    action: Arc<dyn TriggerAction>,
}

impl Trigger {
    pub fn new(id: impl Into<String>, event_kind: impl Into<String>, action: Arc<dyn TriggerAction>) -> Self {
        Self { id: id.into(), event_kind: event_kind.into(), condition: None, action }
    }

    /// Only fire when `condition` returns `true` for the event's payload.
    pub fn with_condition(mut self, condition: impl Fn(&serde_json::Value) -> bool + Send + Sync + 'static) -> Self {
        self.condition = Some(Arc::new(condition));
        self
    }
}

/// Records every event kind that has fired at least once — the satisfaction
/// rule for `Task::depends_on_events` (FEAT-063): once an event has fired,
/// its dependency is permanently satisfied (no "un-firing").
#[derive(Debug, Clone, Default)]
pub struct EventLedger(Arc<Mutex<BTreeSet<String>>>);

impl EventLedger {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&self, event_kind: &str) {
        self.0.lock().unwrap().insert(event_kind.to_string());
    }

    pub fn is_satisfied(&self, event_kind: &str) -> bool {
        self.0.lock().unwrap().contains(event_kind)
    }
}

/// Whether every one of a task's event dependencies has fired at least once
/// (acceptance criterion 1).
pub fn event_dependencies_satisfied(task: &Task, ledger: &EventLedger) -> bool {
    task.depends_on_events.iter().all(|e| ledger.is_satisfied(e))
}

/// The outcome of one `publish` call.
#[derive(Debug, Clone, Default)]
pub struct PublishReport {
    /// Ids of triggers whose action ran successfully.
    pub fired: Vec<String>,
    /// `(trigger id, error message)` for every action that failed —
    /// isolated, not propagated (acceptance criterion 3).
    pub errors: Vec<(String, String)>,
}

/// The event bus: a trigger registry plus the event ledger.
#[derive(Default)]
pub struct EventBus {
    triggers: Vec<Trigger>,
    ledger: EventLedger,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, trigger: Trigger) {
        self.triggers.push(trigger);
    }

    pub fn ledger(&self) -> &EventLedger {
        &self.ledger
    }

    /// Publish an event: records it into the ledger (satisfying any waiting
    /// `depends_on_events`) unconditionally, then runs every bound,
    /// condition-matching trigger. A trigger error is caught and recorded,
    /// never stopping the remaining triggers (acceptance criterion 3).
    pub async fn publish(&self, event: &Event) -> PublishReport {
        self.ledger.record(&event.kind);

        let mut report = PublishReport::default();
        for t in self.triggers.iter().filter(|t| t.event_kind == event.kind) {
            if let Some(cond) = &t.condition
                && !cond(&event.payload)
            {
                continue;
            }
            match t.action.fire(event).await {
                Ok(()) => report.fired.push(t.id.clone()),
                Err(e) => {
                    tracing::warn!(trigger = %t.id, error = %e, "trigger action failed; bus continues");
                    report.errors.push((t.id.clone(), e.to_string()));
                }
            }
        }
        report
    }

    /// Ingest an inbound dvalin bus message: derive an [`Event`] from it
    /// (acceptance criterion 2) and publish it. `Ok(None)` for a message
    /// that isn't event-shaped (unstructured, or a structured body with no
    /// `"kind"` field) — not every bus message is an event, and that's not
    /// an error.
    pub async fn ingest(&self, msg: &BusMessage) -> Result<Option<PublishReport>> {
        match event_from_bus_message(msg)? {
            Some(event) => Ok(Some(self.publish(&event).await)),
            None => Ok(None),
        }
    }
}

/// Map an inbound structured `BusMessage` into an [`Event`], reusing this
/// crate's existing structured-body convention: the body is JSON with its
/// own `"kind"` tag (see `escalation::Escalation`, `chair`'s minutes,
/// `foreman::CaveTask` handovers). The whole body becomes the event
/// payload — a trigger's condition/action can read whatever fields it
/// needs from it.
pub fn event_from_bus_message(msg: &BusMessage) -> Result<Option<Event>> {
    if msg.kind != bus::KIND_STRUCTURED {
        return Ok(None);
    }
    let value: serde_json::Value = match serde_json::from_str(&msg.body) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let kind = match value.get("kind").and_then(|k| k.as_str()) {
        Some(k) => k.to_string(),
        None => return Ok(None),
    };
    Ok(Some(Event::new(kind, value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex as StdMutex;

    struct SpyAction {
        calls: StdMutex<Vec<Event>>,
    }
    impl SpyAction {
        fn new() -> Self {
            Self { calls: StdMutex::new(vec![]) }
        }
    }
    #[async_trait]
    impl TriggerAction for SpyAction {
        async fn fire(&self, event: &Event) -> Result<()> {
            self.calls.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    struct FailingAction;
    #[async_trait]
    impl TriggerAction for FailingAction {
        async fn fire(&self, _event: &Event) -> Result<()> {
            anyhow::bail!("boom")
        }
    }

    fn structured_msg(body: &str) -> BusMessage {
        BusMessage {
            id: 1,
            sender: "dvalin@hq".into(),
            recipient: "regin@cave".into(),
            kind: bus::KIND_STRUCTURED.into(),
            body: body.into(),
            ref_id: None,
            channel: None,
        }
    }

    fn a_task(depends_on_events: &[&str]) -> Task {
        Task {
            id: "t1".into(),
            title: "t1".into(),
            estimated_minutes: 10,
            inputs: vec![],
            outputs: vec![],
            quality_criteria: vec![],
            depends_on_tasks: vec![],
            depends_on_events: depends_on_events.iter().map(|s| s.to_string()).collect(),
            earliest_start: None,
            latest_start: None,
            due: None,
            deadline: None,
            resource_demands: BTreeMap::new(),
        }
    }

    // --- publish + triggers --------------------------------------------

    #[tokio::test]
    async fn publish_invokes_a_bound_trigger_for_a_matching_kind() {
        let spy = Arc::new(SpyAction::new());
        let mut bus = EventBus::new();
        bus.register(Trigger::new("t1", TASK_COMPLETED, spy.clone()));

        let report = bus.publish(&Event::new(TASK_COMPLETED, serde_json::json!({"task_id": "a"}))).await;
        assert_eq!(report.fired, vec!["t1".to_string()]);
        assert_eq!(spy.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn publish_does_not_invoke_a_trigger_bound_to_a_different_kind() {
        let spy = Arc::new(SpyAction::new());
        let mut bus = EventBus::new();
        bus.register(Trigger::new("t1", TASK_FAILED, spy.clone()));

        bus.publish(&Event::new(TASK_COMPLETED, serde_json::Value::Null)).await;
        assert!(spy.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_condition_gates_whether_a_trigger_fires() {
        let spy = Arc::new(SpyAction::new());
        let mut bus = EventBus::new();
        bus.register(
            Trigger::new("t1", OBJECTIVE_BREACHED, spy.clone())
                .with_condition(|p| p.get("severity").and_then(|s| s.as_str()) == Some("high")),
        );

        bus.publish(&Event::new(OBJECTIVE_BREACHED, serde_json::json!({"severity": "low"}))).await;
        assert!(spy.calls.lock().unwrap().is_empty(), "condition doesn't match");

        bus.publish(&Event::new(OBJECTIVE_BREACHED, serde_json::json!({"severity": "high"}))).await;
        assert_eq!(spy.calls.lock().unwrap().len(), 1, "condition matches");
    }

    #[tokio::test]
    async fn multiple_triggers_on_the_same_kind_all_fire() {
        let spy_a = Arc::new(SpyAction::new());
        let spy_b = Arc::new(SpyAction::new());
        let mut bus = EventBus::new();
        bus.register(Trigger::new("a", GOAL_CREATED, spy_a.clone()));
        bus.register(Trigger::new("b", GOAL_CREATED, spy_b.clone()));

        let report = bus.publish(&Event::new(GOAL_CREATED, serde_json::Value::Null)).await;
        assert_eq!(report.fired.len(), 2);
        assert_eq!(spy_a.calls.lock().unwrap().len(), 1);
        assert_eq!(spy_b.calls.lock().unwrap().len(), 1);
    }

    // --- acceptance criterion 1: event -> task dependency satisfaction --

    #[tokio::test]
    async fn publishing_an_event_satisfies_a_matching_task_dependency() {
        let bus = EventBus::new();
        let task = a_task(&["disk.threshold_breached"]);
        assert!(!event_dependencies_satisfied(&task, bus.ledger()));

        bus.publish(&Event::new("disk.threshold_breached", serde_json::Value::Null)).await;
        assert!(event_dependencies_satisfied(&task, bus.ledger()));
    }

    #[tokio::test]
    async fn a_task_with_multiple_event_deps_needs_all_of_them_to_fire() {
        let bus = EventBus::new();
        let task = a_task(&["a.happened", "b.happened"]);

        bus.publish(&Event::new("a.happened", serde_json::Value::Null)).await;
        assert!(!event_dependencies_satisfied(&task, bus.ledger()), "b hasn't fired yet");

        bus.publish(&Event::new("b.happened", serde_json::Value::Null)).await;
        assert!(event_dependencies_satisfied(&task, bus.ledger()));
    }

    #[tokio::test]
    async fn satisfaction_does_not_depend_on_any_trigger_being_registered() {
        let bus = EventBus::new();
        let task = a_task(&["nobody.listens"]);
        // no trigger registered for "nobody.listens" at all
        bus.publish(&Event::new("nobody.listens", serde_json::Value::Null)).await;
        assert!(event_dependencies_satisfied(&task, bus.ledger()));
    }

    // --- acceptance criterion 2: external ingestion ---------------------

    #[test]
    fn event_from_bus_message_maps_a_structured_body_by_its_kind_field() {
        let msg = structured_msg(r#"{"kind":"goal.created","goal_id":"g-1"}"#);
        let event = event_from_bus_message(&msg).unwrap().unwrap();
        assert_eq!(event.kind, "goal.created");
        assert_eq!(event.payload["goal_id"], "g-1");
    }

    #[test]
    fn event_from_bus_message_ignores_unstructured_messages() {
        let msg = BusMessage {
            id: 1, sender: "a".into(), recipient: "b".into(),
            kind: bus::KIND_UNSTRUCTURED.into(), body: "just chatting".into(), ref_id: None, channel: None,
        };
        assert!(event_from_bus_message(&msg).unwrap().is_none());
    }

    #[test]
    fn event_from_bus_message_ignores_a_structured_body_without_a_kind_field() {
        let msg = structured_msg(r#"{"foo":"bar"}"#);
        assert!(event_from_bus_message(&msg).unwrap().is_none());
    }

    #[test]
    fn event_from_bus_message_ignores_malformed_json() {
        let msg = structured_msg("not json");
        assert!(event_from_bus_message(&msg).unwrap().is_none());
    }

    #[tokio::test]
    async fn ingest_publishes_the_derived_event_and_a_bound_trigger_can_start_a_plan() {
        // acceptance criterion 2
        let spy = Arc::new(SpyAction::new());
        let mut bus = EventBus::new();
        bus.register(Trigger::new("instantiate-plan", "goal.created", spy.clone()));

        let msg = structured_msg(r#"{"kind":"goal.created","goal_id":"g-1"}"#);
        let report = bus.ingest(&msg).await.unwrap().expect("event-shaped message");
        assert_eq!(report.fired, vec!["instantiate-plan".to_string()]);
        assert_eq!(spy.calls.lock().unwrap()[0].payload["goal_id"], "g-1");
    }

    #[tokio::test]
    async fn ingest_returns_none_for_a_non_event_message_without_erroring() {
        let bus = EventBus::new();
        let msg = BusMessage {
            id: 1, sender: "a".into(), recipient: "b".into(),
            kind: bus::KIND_UNSTRUCTURED.into(), body: "hi".into(), ref_id: None, channel: None,
        };
        assert!(bus.ingest(&msg).await.unwrap().is_none());
    }

    // --- acceptance criterion 3: trigger errors are isolated -------------

    #[tokio::test]
    async fn a_failing_trigger_is_isolated_and_the_bus_continues() {
        let spy = Arc::new(SpyAction::new());
        let mut bus = EventBus::new();
        bus.register(Trigger::new("bad", TASK_FAILED, Arc::new(FailingAction)));
        bus.register(Trigger::new("good", TASK_FAILED, spy.clone()));

        let report = bus.publish(&Event::new(TASK_FAILED, serde_json::Value::Null)).await;
        assert_eq!(report.fired, vec!["good".to_string()]);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, "bad");
        assert!(report.errors[0].1.contains("boom"));
        assert_eq!(spy.calls.lock().unwrap().len(), 1, "the good trigger still ran");
    }

    #[tokio::test]
    async fn a_failing_trigger_still_lets_the_ledger_record_the_event() {
        let mut bus = EventBus::new();
        bus.register(Trigger::new("bad", "external.thing", Arc::new(FailingAction)));
        let task = a_task(&["external.thing"]);

        bus.publish(&Event::new("external.thing", serde_json::Value::Null)).await;
        assert!(event_dependencies_satisfied(&task, bus.ledger()), "the ledger records before triggers run");
    }
}
