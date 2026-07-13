//! Task executor: polymorphic action + quality-criteria verification
//! (FEAT-065 / DISC-019).
//!
//! Executes one schedule-ready task (FEAT-064) via a **polymorphic
//! action** — a skill invocation (FEAT-045), an LLM sub-agent given the
//! task's inputs + output spec, or a concrete guarded tool call (FEAT-038)
//! — chosen by the *caller* per task, not baked into `Task`'s own schema:
//! FEAT-063 scoped `Task` to the planning artifact (time/inputs/outputs+
//! quality/deps/windows/resources), deliberately not an action kind, so
//! this ticket doesn't reopen that schema.
//!
//! **Gating reuses existing machinery, no parallel policy:**
//! - A [`TaskAction::GuardedOp`] passes `guardrail::check_tool_call`
//!   (FEAT-038's red-lines + capability ceiling) before it runs — a
//!   red-line action is refused outright. A skill/sub-agent action's own
//!   internal tool calls (if any) are gated the same way wherever they
//!   actually dispatch (the live loop) — not re-checked here.
//! - "Significant" vs "trivial" reuses FEAT-028's own
//!   `decision::RiskClassifier`/`ContemplatedAction`/`select_mode`: a
//!   `Deliberate`-classified action consults the Soul
//!   (`decision::SoulGate`) before running; an `Act`-classified one
//!   doesn't. FEAT-068 ("soul gate for intent") is the policy layer that
//!   decides *what counts* as significant for planning-plane actions
//!   specifically — this ticket already wires the real gate mechanism.
//!
//! **Verification reuses `goal::SuccessCriterion`/`evaluate::satisfies`**
//! (the same measurable-preferred/LLM-fallback shape a goal's own success
//! criteria use, and `TaskNetwork::derived_criteria` already carries) —
//! not a parallel verifier. The judge is `goal::GoalJudge`, reused as-is;
//! nothing about judging a fuzzy criterion is goal-specific.
//!
//! Pure-ish engine: [`ActionRunner`] is injectable (mirrors
//! `decision::Planner`/`task_network::TaskPlanner`), so tests never invoke a
//! real skill, LLM, or shell.

use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;

use crate::decision::{ContemplatedAction, Mode, Plan as SoulPlan, RiskClassifier, SoulEvaluation, SoulGate, SoulVerdict, select_mode};
use crate::desired::AssertValue;
use crate::evaluate;
use crate::goal::{GoalJudge, SuccessCriterion};
use crate::guardrail::{self, Decision};
use crate::persona::Persona;
use crate::task_network::Task;
use crate::tools::ToolCall;

/// A task's chosen execution mechanism.
#[derive(Debug, Clone)]
pub enum TaskAction {
    /// Invoke an existing skill (FEAT-045) by name.
    Skill { name: String },
    /// An LLM sub-agent given the task's inputs + output spec as its
    /// prompt.
    SubAgent { prompt: String },
    /// A concrete guarded operation — red-line-bounded before it runs.
    GuardedOp { tool_call: ToolCall },
}

/// Runs a [`TaskAction`] and returns raw observations, keyed to match
/// whatever a task's `Measurable` quality criteria reference. Injectable so
/// tests never need a real skill loader, LLM, or shell (acceptance
/// criterion 1: each action kind executes, verified against fakes).
#[async_trait]
pub trait ActionRunner: Send + Sync {
    async fn run(&self, task: &Task, action: &TaskAction) -> Result<BTreeMap<String, AssertValue>>;
}

/// Why a task didn't reach (or didn't survive) execution, or that it did.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskOutcome {
    /// A red-line/ceiling denial — refused before running.
    Refused { reason: String },
    /// A significant action's Soul vote wasn't `Approve`.
    SoulDenied { reason: String },
    /// Ran; one or more quality criteria didn't hold — hands off to the
    /// planning control loop (FEAT-066).
    Failed { unmet: Vec<String> },
    /// Ran; every quality criterion held (`task.completed`).
    Completed,
}

/// The full result of attempting one task.
#[derive(Debug, Clone)]
pub struct TaskExecutionReport {
    pub task_id: String,
    pub guardrail: Decision,
    /// `Some` iff the action was classified significant enough to consult
    /// the Soul (acceptance criterion 2).
    pub soul: Option<SoulEvaluation>,
    pub outcome: TaskOutcome,
}

impl TaskExecutionReport {
    pub fn completed(&self) -> bool {
        matches!(self.outcome, TaskOutcome::Completed)
    }
}

fn criterion_label(c: &SuccessCriterion) -> String {
    match c {
        SuccessCriterion::Measurable { key, description, .. } => description.clone().unwrap_or_else(|| key.clone()),
        SuccessCriterion::Judged { description } => description.clone(),
    }
}

async fn criterion_holds(c: &SuccessCriterion, task: &Task, observed: &BTreeMap<String, AssertValue>, judge: &dyn GoalJudge) -> Result<bool> {
    Ok(match c {
        SuccessCriterion::Measurable { key, op, value, .. } => match observed.get(key) {
            Some(obs) => evaluate::satisfies(obs, *op, value),
            None => false,
        },
        SuccessCriterion::Judged { description } => judge.holds(&task.title, description).await?,
    })
}

/// Execute one schedule-ready task: guardrail (guarded ops only) -> Soul
/// gate (significant actions only) -> run the action -> verify output
/// against quality criteria. `contemplated` is the caller's risk
/// classification input for this specific action (mirrors
/// `decision::run_deliberate`'s own mode-selection step) — this ticket
/// doesn't invent a second way to derive it.
#[allow(clippy::too_many_arguments)]
pub async fn execute_task(
    task: &Task,
    action: &TaskAction,
    contemplated: &ContemplatedAction,
    persona: Option<&Persona>,
    classifier: &dyn RiskClassifier,
    soul: &dyn SoulGate,
    judge: &dyn GoalJudge,
    runner: &dyn ActionRunner,
) -> Result<TaskExecutionReport> {
    if let TaskAction::GuardedOp { tool_call } = action {
        let decision = guardrail::check_tool_call(tool_call, persona);
        if !decision.is_allowed() {
            let reason = decision.audit().unwrap_or_default();
            return Ok(TaskExecutionReport {
                task_id: task.id.clone(),
                guardrail: decision,
                soul: None,
                outcome: TaskOutcome::Refused { reason },
            });
        }
    }

    let mode = select_mode(contemplated, classifier, persona);
    let soul_eval = if mode == Mode::Deliberate {
        let plan = SoulPlan {
            id: task.id.clone(),
            intent_summary: format!("Task {:?}: {}", task.id, task.title),
            steps: vec![task.title.clone()],
            intended_tool_calls: Vec::new(),
        };
        let eval = soul.evaluate(&plan).await?;
        if eval.verdict != SoulVerdict::Approve {
            return Ok(TaskExecutionReport {
                task_id: task.id.clone(),
                guardrail: Decision::Allow,
                soul: Some(eval.clone()),
                outcome: TaskOutcome::SoulDenied { reason: eval.reaction.clone() },
            });
        }
        Some(eval)
    } else {
        None
    };

    let observed = runner.run(task, action).await?;

    let mut unmet = Vec::new();
    for c in &task.quality_criteria {
        if !criterion_holds(c, task, &observed, judge).await? {
            unmet.push(criterion_label(c));
        }
    }

    let outcome = if unmet.is_empty() { TaskOutcome::Completed } else { TaskOutcome::Failed { unmet } };
    Ok(TaskExecutionReport { task_id: task.id.clone(), guardrail: Decision::Allow, soul: soul_eval, outcome })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{DefaultRiskClassifier, PassthroughSoulGate, RawSoulVerdict};
    use crate::desired::AssertOp;
    use crate::goal::FixedGoalJudge;
    use crate::tools::FunctionCall;
    use std::sync::Mutex;

    fn a_task(quality_criteria: Vec<SuccessCriterion>) -> Task {
        Task {
            id: "t1".into(),
            title: "shrink the disk".into(),
            estimated_minutes: 10,
            inputs: vec![],
            outputs: vec![],
            quality_criteria,
            depends_on_tasks: vec![],
            depends_on_events: vec![],
            earliest_start: None,
            latest_start: None,
            due: None,
            deadline: None,
            resource_demands: BTreeMap::new(),
        }
    }

    fn trivial() -> ContemplatedAction {
        ContemplatedAction { reversible: true, destructive: false, outward_facing: false, urgent: false }
    }

    fn significant() -> ContemplatedAction {
        ContemplatedAction { reversible: false, destructive: true, outward_facing: false, urgent: false }
    }

    fn a_measurable(key: &str, op: AssertOp, value: AssertValue) -> SuccessCriterion {
        SuccessCriterion::Measurable { key: key.into(), op, value, description: Some(format!("{key} check")) }
    }

    struct FixedRunner(BTreeMap<String, AssertValue>);

    #[async_trait]
    impl ActionRunner for FixedRunner {
        async fn run(&self, _task: &Task, _action: &TaskAction) -> Result<BTreeMap<String, AssertValue>> {
            Ok(self.0.clone())
        }
    }

    struct SpyRunner {
        observed: BTreeMap<String, AssertValue>,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ActionRunner for SpyRunner {
        async fn run(&self, task: &Task, _action: &TaskAction) -> Result<BTreeMap<String, AssertValue>> {
            self.calls.lock().unwrap().push(task.id.clone());
            Ok(self.observed.clone())
        }
    }

    struct AlwaysVetoSoul;

    #[async_trait]
    impl SoulGate for AlwaysVetoSoul {
        async fn evaluate(&self, _plan: &SoulPlan) -> Result<SoulEvaluation> {
            Ok(SoulEvaluation {
                verdict: SoulVerdict::Veto,
                reaction: "test double: always vetoes".into(),
                confidence: 1.0,
                raw_verdict: RawSoulVerdict::Veto,
            })
        }
    }

    fn redline_bash() -> TaskAction {
        TaskAction::GuardedOp {
            tool_call: ToolCall {
                id: "call-1".into(),
                call_type: "function".into(),
                function: FunctionCall { name: "bash".into(), arguments: serde_json::json!({"command": "rm -rf /"}).to_string() },
            },
        }
    }

    fn harmless_bash() -> TaskAction {
        TaskAction::GuardedOp {
            tool_call: ToolCall {
                id: "call-2".into(),
                call_type: "function".into(),
                function: FunctionCall { name: "bash".into(), arguments: serde_json::json!({"command": "echo hi"}).to_string() },
            },
        }
    }

    // --- acceptance criterion 1: each action kind executes -----------------

    #[tokio::test]
    async fn a_skill_action_runs_and_completes_when_criteria_hold() {
        let mut observed = BTreeMap::new();
        observed.insert("disk.root.use_percent".to_string(), AssertValue::Num(10.0));
        let task = a_task(vec![a_measurable("disk.root.use_percent", AssertOp::Lt, AssertValue::Num(80.0))]);
        let action = TaskAction::Skill { name: "disk-cleanup".into() };
        let report = execute_task(
            &task, &action, &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(observed),
        ).await.unwrap();
        assert!(report.completed());
    }

    #[tokio::test]
    async fn a_sub_agent_action_runs_and_completes() {
        let task = a_task(vec![]);
        let action = TaskAction::SubAgent { prompt: "clean up disk space".into() };
        let report = execute_task(
            &task, &action, &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(report.completed());
    }

    #[tokio::test]
    async fn a_guarded_op_action_runs_and_completes() {
        let task = a_task(vec![]);
        let report = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(report.completed());
    }

    // --- acceptance criterion 2: guardrail + significance -------------------

    #[tokio::test]
    async fn a_red_line_guarded_op_is_refused_and_never_reaches_the_runner() {
        let task = a_task(vec![]);
        let spy = SpyRunner { observed: BTreeMap::new(), calls: Mutex::new(vec![]) };
        let report = execute_task(
            &task, &redline_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &spy,
        ).await.unwrap();
        assert!(matches!(report.outcome, TaskOutcome::Refused { .. }));
        assert!(!report.guardrail.is_allowed());
        assert!(spy.calls.lock().unwrap().is_empty(), "refused before ever running");
    }

    #[tokio::test]
    async fn a_significant_action_consults_the_soul() {
        let task = a_task(vec![]);
        let report = execute_task(
            &task, &harmless_bash(), &significant(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(report.soul.is_some());
        assert!(report.completed());
    }

    #[tokio::test]
    async fn a_trivial_action_does_not_consult_the_soul() {
        let task = a_task(vec![]);
        let report = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &AlwaysVetoSoul,
            &FixedGoalJudge(true), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(report.soul.is_none(), "trivial actions skip the Soul entirely");
        assert!(report.completed(), "a vetoing Soul that's never consulted can't block a trivial action");
    }

    #[tokio::test]
    async fn a_soul_veto_on_a_significant_action_denies_before_running() {
        let task = a_task(vec![]);
        let spy = SpyRunner { observed: BTreeMap::new(), calls: Mutex::new(vec![]) };
        let report = execute_task(
            &task, &harmless_bash(), &significant(), None, &DefaultRiskClassifier, &AlwaysVetoSoul,
            &FixedGoalJudge(true), &spy,
        ).await.unwrap();
        assert!(matches!(report.outcome, TaskOutcome::SoulDenied { .. }));
        assert!(spy.calls.lock().unwrap().is_empty(), "vetoed before ever running");
    }

    // --- acceptance criterion 3: quality-criteria verification -------------

    #[tokio::test]
    async fn unmet_measurable_criteria_fail_the_task() {
        let mut observed = BTreeMap::new();
        observed.insert("disk.root.use_percent".to_string(), AssertValue::Num(95.0));
        let task = a_task(vec![a_measurable("disk.root.use_percent", AssertOp::Lt, AssertValue::Num(80.0))]);
        let report = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(observed),
        ).await.unwrap();
        match report.outcome {
            TaskOutcome::Failed { unmet } => assert_eq!(unmet, vec!["disk.root.use_percent check".to_string()]),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_missing_observation_never_counts_as_holding() {
        let task = a_task(vec![a_measurable("disk.root.use_percent", AssertOp::Lt, AssertValue::Num(80.0))]);
        let report = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(!report.completed());
    }

    #[tokio::test]
    async fn a_judged_criterion_uses_the_injected_judge() {
        let task = a_task(vec![SuccessCriterion::Judged { description: "output reads cleanly".into() }]);
        let denied = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(false), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(!denied.completed());

        let approved = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(approved.completed());
    }

    #[tokio::test]
    async fn every_criterion_must_hold_mixed_measurable_and_judged() {
        let mut observed = BTreeMap::new();
        observed.insert("disk.root.use_percent".to_string(), AssertValue::Num(10.0));
        let task = a_task(vec![
            a_measurable("disk.root.use_percent", AssertOp::Lt, AssertValue::Num(80.0)),
            SuccessCriterion::Judged { description: "looks good".into() },
        ]);
        let report = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(false), &FixedRunner(observed),
        ).await.unwrap();
        assert!(!report.completed(), "measurable holds but the judged criterion doesn't");
    }

    #[tokio::test]
    async fn no_quality_criteria_completes_trivially() {
        let task = a_task(vec![]);
        let report = execute_task(
            &task, &harmless_bash(), &trivial(), None, &DefaultRiskClassifier, &PassthroughSoulGate,
            &FixedGoalJudge(true), &FixedRunner(BTreeMap::new()),
        ).await.unwrap();
        assert!(report.completed());
    }
}
