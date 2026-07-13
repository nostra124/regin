//! Soul gate for intent (FEAT-068, extended by FEAT-069 / DISC-019).
//!
//! Routes the identity plane's Soul gate (FEAT-029: `decision::SoulGate`)
//! through the three checkpoints DISC-019 calls for, reusing existing
//! mechanism at each rather than building a parallel gate:
//!
//! 1. **Goals & objectives** — [`goal_create_gated`] /
//!    [`objective_create_gated`] ask "does this intent fit our values?"
//!    before an intent is ever persisted (`goal::goal_create` /
//!    `objective::objective_create`). Neither create fn had a Soul
//!    checkpoint at all before FEAT-068/069; this is the authorship
//!    approval-gate FEAT-069's acceptance criterion 1 asks for.
//! 2. **Plans** — [`gate_plan`] wraps `task_network::plan_and_gate`
//!    (FEAT-063, already Soul-gated) and adds deliberation capture on top,
//!    so a plan's acceptance/rejection is recorded the same way a goal's
//!    is.
//! 3. **Significant actions at execution time** — already fully built by
//!    `task_executor::execute_task` (FEAT-065): significance
//!    (`decision::RiskClassifier`/`ContemplatedAction`) decides whether the
//!    Soul is consulted at all, and `guardrail::check_tool_call`'s
//!    red-lines are checked independently of significance, before the Soul
//!    ever sees the action. Nothing to add here — this module's tests
//!    exercise it directly to keep "all three checkpoints, one soul
//!    mechanism" documented and verified in one place.
//!
//! **Rejections are recorded, not silently swallowed** (acceptance
//! criterion 1): both gates capture a `decision::DeliberationRecord` via an
//! injected `DeliberationSink` (FEAT-032's existing audit mechanism, not a
//! new one) regardless of verdict — best-effort, mirroring
//! `decision::run_deliberate`'s own capture discipline: a capture failure
//! is logged and never blocks or fails the gate itself.
//!
//! A goal-creation rejection is a single-shot Denied — there is no
//! multi-round revise loop for a goal's free-text description the way
//! `decision::run_deliberate` has for an action Plan; `RawSoulVerdict::Revise`
//! is treated the same as `Veto` here (blocked, with the reaction as the
//! reason).

use anyhow::Result;
use rusqlite::Connection;

use crate::decision::{DeliberationRecord, DeliberationSink, Disposition, Plan as SoulPlan, SoulEvaluation, SoulGate, SoulVerdict};
use crate::desired::AssertValue;
use crate::goal::{self, Goal, SuccessCriterion};
use crate::objective::{self, Objective};
use crate::task_network::{self, PlannedNetwork, TaskPlanner};

/// The result of a Soul-gated intent checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome<T> {
    Approved(T),
    /// Blocked — the reason is the Soul's own reaction (also captured via
    /// the `DeliberationSink`).
    Rejected { reason: String },
}

impl<T> GateOutcome<T> {
    pub fn approved(&self) -> bool {
        matches!(self, GateOutcome::Approved(_))
    }
}

fn capture_deliberation(
    sink: &dyn DeliberationSink,
    plan_id: &str,
    intent_summary: &str,
    steps: &[String],
    eval: &SoulEvaluation,
    disposition: Disposition,
) {
    let record = DeliberationRecord {
        plan_id: plan_id.to_string(),
        intent_summary: intent_summary.to_string(),
        steps: steps.to_vec(),
        confidence: eval.confidence,
        verdict: eval.raw_verdict,
        gut_reaction: eval.reaction.clone(),
        disposition,
        outcome: None,
        outcome_ref_id: None,
    };
    if let Err(e) = sink.capture(&record) {
        tracing::warn!(plan_id, error = %e, "failed to capture a Soul deliberation record (best-effort)");
    }
}

/// Checkpoint 1: gate a goal's creation on the Soul before it's ever
/// persisted (acceptance criterion 1). A rejection creates nothing — same
/// no-partial-writes convention `goal::goal_create`'s own validation
/// already follows.
#[allow(clippy::too_many_arguments)]
pub async fn goal_create_gated(
    conn: &Connection,
    description: &str,
    target: &str,
    deadline: &str,
    criteria: Vec<SuccessCriterion>,
    priority: i64,
    source: &str,
    soul: &dyn SoulGate,
    sink: &dyn DeliberationSink,
) -> Result<GateOutcome<Goal>> {
    let plan_id = uuid::Uuid::new_v4().to_string();
    let intent_summary = format!("New goal: {description:?} (target: {target:?}, deadline: {deadline})");
    let plan = SoulPlan { id: plan_id.clone(), intent_summary: intent_summary.clone(), steps: vec![], intended_tool_calls: vec![] };

    let eval = soul.evaluate(&plan).await?;
    let disposition = if eval.verdict == SoulVerdict::Approve { Disposition::Executed } else { Disposition::Denied };
    capture_deliberation(sink, &plan_id, &intent_summary, &[], &eval, disposition);

    if eval.verdict != SoulVerdict::Approve {
        return Ok(GateOutcome::Rejected { reason: eval.reaction });
    }

    let goal = goal::goal_create(conn, description, target, deadline, criteria, priority, source)?;
    Ok(GateOutcome::Approved(goal))
}

/// Checkpoint 1, objective flavour (FEAT-069 acceptance criterion 1):
/// `objective::objective_create` had no Soul checkpoint either — the same
/// gap `goal_create_gated` closed for goals.
#[allow(clippy::too_many_arguments)]
pub async fn objective_create_gated(
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
    soul: &dyn SoulGate,
    sink: &dyn DeliberationSink,
) -> Result<GateOutcome<Objective>> {
    let plan_id = uuid::Uuid::new_v4().to_string();
    let intent_summary = format!("New objective: {title:?} ({description:?})");
    let plan = SoulPlan { id: plan_id.clone(), intent_summary: intent_summary.clone(), steps: vec![], intended_tool_calls: vec![] };

    let eval = soul.evaluate(&plan).await?;
    let disposition = if eval.verdict == SoulVerdict::Approve { Disposition::Executed } else { Disposition::Denied };
    capture_deliberation(sink, &plan_id, &intent_summary, &[], &eval, disposition);

    if eval.verdict != SoulVerdict::Approve {
        return Ok(GateOutcome::Rejected { reason: eval.reaction });
    }

    let objective = objective::objective_create(conn, title, description, metric, aggregate, window_days, op, value, priority, source)?;
    Ok(GateOutcome::Approved(objective))
}

/// Checkpoint 2: plan a goal into a task network via `task_network::
/// plan_and_gate` (FEAT-063, already Soul-gated) and additionally capture
/// the deliberation (acceptance criterion 1's "plan activation" half) —
/// this ticket's contribution is the audit trail, not a second gate.
pub async fn gate_plan(
    goal: &Goal,
    revision_feedback: Option<&str>,
    planner: &dyn TaskPlanner,
    soul: &dyn SoulGate,
    sink: &dyn DeliberationSink,
) -> Result<PlannedNetwork> {
    let planned = task_network::plan_and_gate(goal, revision_feedback, planner, soul).await?;

    let steps: Vec<String> = planned.network.tasks.iter().map(|t| t.title.clone()).collect();
    let intent_summary = format!("Plan toward goal {:?}: {} task(s)", goal.description, planned.network.tasks.len());
    let disposition = if planned.approved() { Disposition::Executed } else { Disposition::Denied };
    capture_deliberation(sink, &planned.network.id, &intent_summary, &steps, &planned.soul, disposition);

    Ok(planned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::decision::{PassthroughSoulGate, RawSoulVerdict};
    use crate::task_executor::{self, ActionRunner, TaskAction, TaskOutcome};
    use crate::task_network::{Task, TaskNetwork};
    use async_trait::async_trait;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        db::init_schema(&c).unwrap();
        c
    }

    fn future_deadline(days: i64) -> String {
        (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339()
    }

    struct FixedVerdictSoul(SoulVerdict);
    #[async_trait]
    impl SoulGate for FixedVerdictSoul {
        async fn evaluate(&self, _plan: &SoulPlan) -> Result<SoulEvaluation> {
            let raw = match self.0 {
                SoulVerdict::Approve => RawSoulVerdict::Approve,
                SoulVerdict::Revise => RawSoulVerdict::Revise,
                SoulVerdict::Veto => RawSoulVerdict::Veto,
            };
            Ok(SoulEvaluation { verdict: self.0, reaction: "test double verdict".into(), confidence: 0.9, raw_verdict: raw })
        }
    }

    struct SpySink {
        records: Mutex<Vec<DeliberationRecord>>,
    }
    impl SpySink {
        fn new() -> Self {
            Self { records: Mutex::new(vec![]) }
        }
    }
    impl DeliberationSink for SpySink {
        fn capture(&self, record: &DeliberationRecord) -> Result<String> {
            self.records.lock().unwrap().push(record.clone());
            Ok("captured".into())
        }
    }

    struct FailingSink;
    impl DeliberationSink for FailingSink {
        fn capture(&self, _record: &DeliberationRecord) -> Result<String> {
            anyhow::bail!("disk full")
        }
    }

    struct FixedPlanner(TaskNetwork);
    #[async_trait]
    impl TaskPlanner for FixedPlanner {
        async fn plan(&self, _goal: &Goal, _revision_feedback: Option<&str>) -> Result<TaskNetwork> {
            Ok(self.0.clone())
        }
    }

    fn a_network() -> TaskNetwork {
        TaskNetwork {
            id: "net-1".into(),
            goal_id: "goal-1".into(),
            tasks: vec![Task {
                id: "t1".into(), title: "clean up disk".into(), estimated_minutes: 10,
                inputs: vec![], outputs: vec![], quality_criteria: vec![],
                depends_on_tasks: vec![], depends_on_events: vec![],
                earliest_start: None, latest_start: None, due: None, deadline: None,
                resource_demands: BTreeMap::new(),
            }],
            derived_criteria: vec![],
        }
    }

    fn a_goal() -> Goal {
        Goal {
            id: "goal-1".into(), description: "shrink disk usage".into(), target: "root under 80%".into(),
            deadline: "2027-01-01T00:00:00Z".into(), criteria: vec![], priority: 1, source: "human".into(),
            rag: "green".into(), status: "active".into(), created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    // --- checkpoint 1: goal creation -----------------------------------

    #[tokio::test]
    async fn goal_create_gated_creates_the_goal_when_the_soul_approves() {
        let c = conn();
        let sink = SpySink::new();
        let outcome = goal_create_gated(&c, "d", "t", &future_deadline(30), vec![], 1, "human", &PassthroughSoulGate, &sink).await.unwrap();
        assert!(outcome.approved());
        assert_eq!(goal::goal_list(&c, None).unwrap().len(), 1);
        assert_eq!(sink.records.lock().unwrap()[0].disposition, Disposition::Executed);
    }

    #[tokio::test]
    async fn goal_create_gated_blocks_creation_and_records_the_reason_when_the_soul_rejects() {
        // acceptance criterion 1
        let c = conn();
        let sink = SpySink::new();
        let soul = FixedVerdictSoul(SoulVerdict::Veto);
        let outcome = goal_create_gated(&c, "d", "t", &future_deadline(30), vec![], 1, "human", &soul, &sink).await.unwrap();

        match outcome {
            GateOutcome::Rejected { reason } => assert_eq!(reason, "test double verdict"),
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert_eq!(goal::goal_list(&c, None).unwrap().len(), 0, "a rejected goal is never persisted");
        let records = sink.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].disposition, Disposition::Denied);
        assert_eq!(records[0].gut_reaction, "test double verdict");
    }

    #[tokio::test]
    async fn goal_create_gated_treats_revise_as_denied_single_shot() {
        let c = conn();
        let sink = SpySink::new();
        let soul = FixedVerdictSoul(SoulVerdict::Revise);
        let outcome = goal_create_gated(&c, "d", "t", &future_deadline(30), vec![], 1, "human", &soul, &sink).await.unwrap();
        assert!(!outcome.approved());
        assert_eq!(goal::goal_list(&c, None).unwrap().len(), 0);
    }

    #[tokio::test]
    async fn a_deliberation_capture_failure_never_blocks_goal_creation() {
        let c = conn();
        let outcome = goal_create_gated(&c, "d", "t", &future_deadline(30), vec![], 1, "human", &PassthroughSoulGate, &FailingSink).await.unwrap();
        assert!(outcome.approved(), "best-effort capture failure doesn't block the gate");
        assert_eq!(goal::goal_list(&c, None).unwrap().len(), 1);
    }

    // --- checkpoint 1, objective flavour (FEAT-069 acceptance criterion 1) --

    #[tokio::test]
    async fn objective_create_gated_creates_the_objective_when_the_soul_approves() {
        let c = conn();
        let sink = SpySink::new();
        let outcome = objective_create_gated(
            &c, "t", "d", "m", "sum", 30, "le", &AssertValue::Num(1.0), 1, "human", &PassthroughSoulGate, &sink,
        ).await.unwrap();
        assert!(outcome.approved());
        assert_eq!(objective::objective_list(&c).unwrap().len(), 1);
        assert_eq!(sink.records.lock().unwrap()[0].disposition, Disposition::Executed);
    }

    #[tokio::test]
    async fn objective_create_gated_blocks_creation_and_records_the_reason_when_the_soul_rejects() {
        let c = conn();
        let sink = SpySink::new();
        let soul = FixedVerdictSoul(SoulVerdict::Veto);
        let outcome = objective_create_gated(
            &c, "t", "d", "m", "sum", 30, "le", &AssertValue::Num(1.0), 1, "human", &soul, &sink,
        ).await.unwrap();

        match outcome {
            GateOutcome::Rejected { reason } => assert_eq!(reason, "test double verdict"),
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert_eq!(objective::objective_list(&c).unwrap().len(), 0, "a rejected objective is never persisted");
        assert_eq!(sink.records.lock().unwrap()[0].disposition, Disposition::Denied);
    }

    // --- checkpoint 2: plan activation ----------------------------------

    #[tokio::test]
    async fn gate_plan_approves_and_records_when_the_soul_approves() {
        let sink = SpySink::new();
        let planned = gate_plan(&a_goal(), None, &FixedPlanner(a_network()), &PassthroughSoulGate, &sink).await.unwrap();
        assert!(planned.approved());
        let records = sink.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].disposition, Disposition::Executed);
        assert_eq!(records[0].steps, vec!["clean up disk".to_string()]);
    }

    #[tokio::test]
    async fn gate_plan_blocks_and_records_the_reason_when_the_soul_rejects() {
        // acceptance criterion 1
        let sink = SpySink::new();
        let soul = FixedVerdictSoul(SoulVerdict::Veto);
        let planned = gate_plan(&a_goal(), None, &FixedPlanner(a_network()), &soul, &sink).await.unwrap();
        assert!(!planned.approved());
        let records = sink.records.lock().unwrap();
        assert_eq!(records[0].disposition, Disposition::Denied);
        assert_eq!(records[0].gut_reaction, "test double verdict");
    }

    #[tokio::test]
    async fn a_deliberation_capture_failure_never_blocks_plan_gating() {
        let planned = gate_plan(&a_goal(), None, &FixedPlanner(a_network()), &PassthroughSoulGate, &FailingSink).await.unwrap();
        assert!(planned.approved());
    }

    #[tokio::test]
    async fn gate_plan_forwards_revision_feedback_to_the_planner() {
        struct SpyPlanner(Mutex<Vec<Option<String>>>);
        #[async_trait]
        impl TaskPlanner for SpyPlanner {
            async fn plan(&self, _goal: &Goal, revision_feedback: Option<&str>) -> Result<TaskNetwork> {
                self.0.lock().unwrap().push(revision_feedback.map(|s| s.to_string()));
                Ok(a_network())
            }
        }
        let planner = SpyPlanner(Mutex::new(vec![]));
        let sink = SpySink::new();
        gate_plan(&a_goal(), Some("prior attempt failed"), &planner, &PassthroughSoulGate, &sink).await.unwrap();
        assert_eq!(planner.0.lock().unwrap().as_slice(), &[Some("prior attempt failed".to_string())]);
    }

    // --- checkpoint 3: significant actions at execution time (AC2) ------
    // Already implemented by task_executor::execute_task (FEAT-065); these
    // tests exercise it directly so the "three checkpoints, one mechanism"
    // story is verified in one place.

    struct NoopRunner;
    #[async_trait]
    impl ActionRunner for NoopRunner {
        async fn run(&self, _task: &Task, _action: &TaskAction) -> Result<BTreeMap<String, AssertValue>> {
            Ok(BTreeMap::new())
        }
    }

    fn a_task() -> Task {
        Task {
            id: "t1".into(), title: "restart service".into(), estimated_minutes: 5,
            inputs: vec![], outputs: vec![], quality_criteria: vec![],
            depends_on_tasks: vec![], depends_on_events: vec![],
            earliest_start: None, latest_start: None, due: None, deadline: None,
            resource_demands: BTreeMap::new(),
        }
    }

    fn harmless_bash() -> TaskAction {
        TaskAction::GuardedOp {
            tool_call: crate::tools::ToolCall {
                id: "1".into(), call_type: "function".into(),
                function: crate::tools::FunctionCall { name: "bash".into(), arguments: serde_json::json!({"command": "echo hi"}).to_string() },
            },
        }
    }

    #[tokio::test]
    async fn checkpoint_3_a_significant_action_is_soul_checked_at_execution() {
        let significant = crate::decision::ContemplatedAction { reversible: false, destructive: true, outward_facing: false, urgent: false };
        let report = task_executor::execute_task(
            &a_task(), &harmless_bash(), &significant, None, &crate::decision::DefaultRiskClassifier,
            &PassthroughSoulGate, &crate::goal::FixedGoalJudge(true), &NoopRunner,
        ).await.unwrap();
        assert!(report.soul.is_some());
    }

    #[tokio::test]
    async fn checkpoint_3_a_trivial_action_skips_the_soul() {
        let trivial = crate::decision::ContemplatedAction { reversible: true, destructive: false, outward_facing: false, urgent: false };
        let report = task_executor::execute_task(
            &a_task(), &harmless_bash(), &trivial, None, &crate::decision::DefaultRiskClassifier,
            &PassthroughSoulGate, &crate::goal::FixedGoalJudge(true), &NoopRunner,
        ).await.unwrap();
        assert!(report.soul.is_none());
    }

    #[tokio::test]
    async fn checkpoint_3_red_lines_apply_regardless_of_significance() {
        // even a "trivial" contemplated action is refused if it's a red-line —
        // the guardrail is orthogonal to significance (acceptance criterion 2).
        let trivial = crate::decision::ContemplatedAction { reversible: true, destructive: false, outward_facing: false, urgent: false };
        let redline = TaskAction::GuardedOp {
            tool_call: crate::tools::ToolCall {
                id: "1".into(), call_type: "function".into(),
                function: crate::tools::FunctionCall { name: "bash".into(), arguments: serde_json::json!({"command": "rm -rf /"}).to_string() },
            },
        };
        let report = task_executor::execute_task(
            &a_task(), &redline, &trivial, None, &crate::decision::DefaultRiskClassifier,
            &PassthroughSoulGate, &crate::goal::FixedGoalJudge(true), &NoopRunner,
        ).await.unwrap();
        assert!(matches!(report.outcome, TaskOutcome::Refused { .. }));
        assert!(!report.guardrail.is_allowed());
    }
}
