//! Planning control loop: mitigate -> replan -> RAG -> escalate
//! (FEAT-066 / DISC-019).
//!
//! Keeps a goal's plan on track and surfaces trouble honestly. Task failure
//! is a **planning-domain** loop — deliberately never an ITIL incident (the
//! escalation payload here is its own `PlanningEscalation`, unrelated to
//! `escalation::Escalation`, which is specifically the ITIL-problem->dvalin
//! bug/feat bridge, FEAT-015): **mitigate** (retry the task in place) first;
//! if that doesn't clear every failure, **replan** (reuses
//! `task_network::plan_and_gate`, now threading `revision_feedback` — the
//! re-entrancy FEAT-063's own doc comment already promised this ticket).
//!
//! RAG is computed from **both** the schedule's structural feasibility
//! (FEAT-064) and this round's actual recovery outcome — not schedule shape
//! alone, since "off-track but mitigated" is a statement about *this
//! round's history*, not something the schedule can see by itself:
//! - 🔴 **Red**: the schedule is infeasible, or a failure survived both
//!   mitigation and replanning — endangered.
//! - 🟡 **Amber**: the schedule is feasible and nothing is still failing,
//!   but this round required mitigation/replanning to get there —
//!   off-track, recovered, not endangered.
//! - 🟢 **Green**: feasible with nothing to recover from this round.
//!
//! On red, escalates to the goal's `source` (`objective::IntentSource`,
//! shared intent vocabulary) with the three DISC-019 remedies: provide
//! resources, adjust the goal, or replan. `EscalationSink` is injectable —
//! wiring the real channel (email/CLI/bus, source-routed) is FEAT-069's
//! job, not this ticket's.
//!
//! Scoped to **goals**: an objective's RAG already comes from FEAT-060's
//! `objective::check_objectives` (a KPI-breach loop) — objectives aren't
//! decomposed into task networks, so there's nothing here for them to
//! mitigate or replan.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::decision::SoulGate;
use crate::goal::Goal;
use crate::objective::Rag;
use crate::rcpsp::{self, ScheduleInput, ScheduleReport};
use crate::task_executor::TaskExecutionReport;
use crate::task_network::{self, Task, TaskPlanner};

/// Attempts to recover one failed task in place (retry / alternative path)
/// without regenerating the whole network. Injectable so tests never need a
/// real retry mechanism. `Ok(None)` means no mitigation was available or
/// attempted — the caller falls through to replanning.
#[async_trait]
pub trait Mitigator: Send + Sync {
    async fn mitigate(&self, task: &Task) -> Result<Option<TaskExecutionReport>>;
}

/// A remedy the escalated-to source can apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Remedy {
    ProvideResources,
    AdjustIntent,
    Replan,
}

/// The three DISC-019 remedies, always offered together.
pub fn standard_remedies() -> Vec<Remedy> {
    vec![Remedy::ProvideResources, Remedy::AdjustIntent, Remedy::Replan]
}

/// A planning-domain escalation — distinct from `escalation::Escalation`
/// (the ITIL problem->dvalin bridge); this never touches the ITIL store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanningEscalation {
    pub goal_id: String,
    /// The goal's own `source` (`objective::IntentSource`) — who this
    /// routes back to (FEAT-069 owns the actual routing).
    pub source: String,
    pub reason: String,
    pub remedies: Vec<Remedy>,
}

/// Where an escalation goes. Injectable so tests use a spy instead of a
/// real channel — FEAT-069 wires the source-routed one.
#[async_trait]
pub trait EscalationSink: Send + Sync {
    async fn escalate(&self, escalation: &PlanningEscalation) -> Result<()>;
}

/// The full result of one control-loop pass.
#[derive(Debug, Clone)]
pub struct ControlLoopReport {
    pub rag: Rag,
    pub schedule: ScheduleReport,
    /// Ids of tasks a `Mitigator` recovered in place this round.
    pub mitigated_tasks: Vec<String>,
    /// Whether a replan ran this round (attempted; not necessarily approved
    /// by the Soul — see `replan_approved`).
    pub replanned: bool,
    /// `Some(true/false)` iff a replan ran, recording the Soul's verdict on
    /// it; `None` if no replan was needed.
    pub replan_approved: Option<bool>,
    /// Ids of tasks that failed and could not be recovered this round.
    pub still_failed: Vec<String>,
    pub escalation: Option<PlanningEscalation>,
}

/// Derives RAG from the schedule's structural feasibility plus this round's
/// recovery outcome (acceptance criterion 1). A schedule with no failures
/// to report is `still_failed: &[]`, `recovered_this_round: false`.
pub fn compute_rag(schedule: &ScheduleReport, still_failed: &[String], recovered_this_round: bool) -> Rag {
    if !schedule.feasible || !still_failed.is_empty() {
        Rag::Red
    } else if recovered_this_round {
        Rag::Amber
    } else {
        Rag::Green
    }
}

/// Run one control-loop pass for a goal whose scheduled plan has
/// `failed_task_ids` failing. Mitigates each in place first; anything still
/// failing triggers a whole-network replan (acceptance criterion 2). RAG is
/// recomputed from the outcome; a still-red result escalates to the goal's
/// source with the three remedies (acceptance criterion 3).
#[allow(clippy::too_many_arguments)]
pub async fn run_control_loop(
    goal: &Goal,
    schedule_base: &ScheduleInput,
    failed_task_ids: &[String],
    planner: &dyn TaskPlanner,
    soul: &dyn SoulGate,
    mitigator: &dyn Mitigator,
    escalation_sink: &dyn EscalationSink,
) -> Result<ControlLoopReport> {
    let mut schedule = rcpsp::schedule(schedule_base)?;

    let mut mitigated_tasks = Vec::new();
    let mut still_failed = Vec::new();
    for task_id in failed_task_ids {
        let task = schedule_base
            .tasks
            .iter()
            .find(|t| &t.id == task_id)
            .with_context(|| format!("no task {task_id:?} in the schedule input"))?;
        match mitigator.mitigate(task).await? {
            Some(report) if report.completed() => mitigated_tasks.push(task_id.clone()),
            _ => still_failed.push(task_id.clone()),
        }
    }

    let mut replanned = false;
    let mut replan_approved = None;
    if !still_failed.is_empty() {
        let feedback = format!("tasks failed and could not be mitigated: {}", still_failed.join(", "));
        let planned = task_network::plan_and_gate(goal, Some(&feedback), planner, soul).await?;
        replanned = true;
        replan_approved = Some(planned.approved());
        if planned.approved() {
            let mut new_input = schedule_base.clone();
            new_input.tasks = planned.network.tasks;
            schedule = rcpsp::schedule(&new_input)?;
            still_failed.clear();
        }
    }

    let recovered_this_round = !mitigated_tasks.is_empty() || replanned;
    let rag = compute_rag(&schedule, &still_failed, recovered_this_round);

    let escalation = if rag == Rag::Red {
        let reason = if !still_failed.is_empty() {
            format!("tasks still failing after mitigate/replan: {}", still_failed.join(", "))
        } else {
            format!("schedule is infeasible: {}", schedule.issues.join("; "))
        };
        let esc = PlanningEscalation { goal_id: goal.id.clone(), source: goal.source.clone(), reason, remedies: standard_remedies() };
        escalation_sink.escalate(&esc).await?;
        Some(esc)
    } else {
        None
    };

    Ok(ControlLoopReport { rag, schedule, mitigated_tasks, replanned, replan_approved, still_failed, escalation })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{PassthroughSoulGate, SoulEvaluation, SoulVerdict, RawSoulVerdict, Plan as SoulPlan};
    use crate::task_executor::TaskOutcome;
    use crate::task_network::TaskNetwork;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    fn a_task(id: &str) -> Task {
        Task {
            id: id.into(),
            title: id.into(),
            estimated_minutes: 10,
            inputs: vec![],
            outputs: vec![],
            quality_criteria: vec![],
            depends_on_tasks: vec![],
            depends_on_events: vec![],
            earliest_start: None,
            latest_start: None,
            due: None,
            deadline: None,
            resource_demands: BTreeMap::new(),
        }
    }

    fn plan_start() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc)
    }

    fn schedule_input(tasks: Vec<Task>) -> ScheduleInput {
        ScheduleInput {
            plan_start: plan_start(),
            tasks,
            max_concurrency: 100,
            resource_capacities: BTreeMap::new(),
            cost_budget: None,
            deadline: None,
        }
    }

    fn a_goal() -> Goal {
        Goal {
            id: "goal-1".into(),
            description: "shrink disk usage".into(),
            target: "root under 80%".into(),
            deadline: "2027-01-01T00:00:00Z".into(),
            criteria: vec![],
            priority: 1,
            source: "human".into(),
            rag: "green".into(),
            status: "active".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    struct NeverMitigates;
    #[async_trait]
    impl Mitigator for NeverMitigates {
        async fn mitigate(&self, _task: &Task) -> Result<Option<TaskExecutionReport>> {
            Ok(None)
        }
    }

    struct AlwaysMitigates;
    #[async_trait]
    impl Mitigator for AlwaysMitigates {
        async fn mitigate(&self, task: &Task) -> Result<Option<TaskExecutionReport>> {
            Ok(Some(TaskExecutionReport { task_id: task.id.clone(), guardrail: crate::guardrail::Decision::Allow, soul: None, outcome: TaskOutcome::Completed }))
        }
    }

    struct FixedPlanner(TaskNetwork);
    #[async_trait]
    impl TaskPlanner for FixedPlanner {
        async fn plan(&self, _goal: &Goal, _revision_feedback: Option<&str>) -> Result<TaskNetwork> {
            Ok(self.0.clone())
        }
    }

    struct AlwaysVetoSoul;
    #[async_trait]
    impl SoulGate for AlwaysVetoSoul {
        async fn evaluate(&self, _plan: &SoulPlan) -> Result<SoulEvaluation> {
            Ok(SoulEvaluation { verdict: SoulVerdict::Veto, reaction: "no".into(), confidence: 1.0, raw_verdict: RawSoulVerdict::Veto })
        }
    }

    struct SpySink {
        calls: Mutex<Vec<PlanningEscalation>>,
    }
    #[async_trait]
    impl EscalationSink for SpySink {
        async fn escalate(&self, escalation: &PlanningEscalation) -> Result<()> {
            self.calls.lock().unwrap().push(escalation.clone());
            Ok(())
        }
    }

    fn a_network(tasks: Vec<Task>) -> TaskNetwork {
        TaskNetwork { id: "net-1".into(), goal_id: "goal-1".into(), tasks, derived_criteria: vec![] }
    }

    // --- acceptance criterion 1: RAG from the schedule ----------------------

    #[test]
    fn compute_rag_is_green_when_feasible_and_nothing_to_recover() {
        let input = schedule_input(vec![a_task("a")]);
        let report = rcpsp::schedule(&input).unwrap();
        assert_eq!(compute_rag(&report, &[], false), Rag::Green);
    }

    #[test]
    fn compute_rag_is_amber_when_feasible_but_recovered_this_round() {
        let input = schedule_input(vec![a_task("a")]);
        let report = rcpsp::schedule(&input).unwrap();
        assert_eq!(compute_rag(&report, &[], true), Rag::Amber);
    }

    #[test]
    fn compute_rag_is_red_when_infeasible_regardless_of_recovery() {
        let mut a = a_task("a");
        a.estimated_minutes = 100;
        let mut input = schedule_input(vec![a]);
        input.deadline = Some(plan_start() + chrono::Duration::minutes(10));
        let report = rcpsp::schedule(&input).unwrap();
        assert!(!report.feasible);
        assert_eq!(compute_rag(&report, &[], true), Rag::Red);
    }

    #[test]
    fn compute_rag_is_red_when_a_task_is_still_failing() {
        let input = schedule_input(vec![a_task("a")]);
        let report = rcpsp::schedule(&input).unwrap();
        assert_eq!(compute_rag(&report, &["a".to_string()], false), Rag::Red);
    }

    // --- acceptance criterion 2: mitigate -> replan -------------------------

    #[tokio::test]
    async fn a_mitigated_task_recovers_to_amber_without_replanning() {
        let input = schedule_input(vec![a_task("a")]);
        let goal = a_goal();
        let sink = SpySink { calls: Mutex::new(vec![]) };
        let report = run_control_loop(
            &goal, &input, &["a".to_string()], &FixedPlanner(a_network(vec![])), &PassthroughSoulGate,
            &AlwaysMitigates, &sink,
        ).await.unwrap();

        assert_eq!(report.mitigated_tasks, vec!["a".to_string()]);
        assert!(!report.replanned);
        assert!(report.still_failed.is_empty());
        assert_eq!(report.rag, Rag::Amber);
        assert!(report.escalation.is_none());
        assert!(sink.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_unmitigatable_task_triggers_a_replan_that_recovers() {
        let input = schedule_input(vec![a_task("a")]);
        let goal = a_goal();
        let replanned_network = a_network(vec![a_task("b")]); // the regenerated plan drops the failing task
        let sink = SpySink { calls: Mutex::new(vec![]) };
        let report = run_control_loop(
            &goal, &input, &["a".to_string()], &FixedPlanner(replanned_network), &PassthroughSoulGate,
            &NeverMitigates, &sink,
        ).await.unwrap();

        assert!(report.mitigated_tasks.is_empty());
        assert!(report.replanned);
        assert_eq!(report.replan_approved, Some(true));
        assert!(report.still_failed.is_empty(), "the replan recovered — not still failing");
        assert_eq!(report.rag, Rag::Amber, "recovered via replan, not endangered");
        assert!(report.escalation.is_none());
        assert_eq!(report.schedule.tasks[0].task_id, "b", "the new network's schedule is what's reported");
    }

    // --- acceptance criterion 3: endangered -> escalate with 3 remedies ----

    #[tokio::test]
    async fn a_vetoed_replan_leaves_the_task_failing_goes_red_and_escalates() {
        let input = schedule_input(vec![a_task("a")]);
        let goal = a_goal();
        let sink = SpySink { calls: Mutex::new(vec![]) };
        let report = run_control_loop(
            &goal, &input, &["a".to_string()], &FixedPlanner(a_network(vec![a_task("a")])), &AlwaysVetoSoul,
            &NeverMitigates, &sink,
        ).await.unwrap();

        assert_eq!(report.replan_approved, Some(false));
        assert_eq!(report.still_failed, vec!["a".to_string()]);
        assert_eq!(report.rag, Rag::Red);
        let esc = report.escalation.expect("red must escalate");
        assert_eq!(esc.goal_id, goal.id);
        assert_eq!(esc.source, goal.source);
        assert_eq!(esc.remedies, standard_remedies());
        assert_eq!(sink.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_infeasible_schedule_with_no_failed_tasks_still_escalates() {
        let mut a = a_task("a");
        a.estimated_minutes = 100;
        let mut input = schedule_input(vec![a]);
        input.deadline = Some(plan_start() + chrono::Duration::minutes(10));
        let goal = a_goal();
        let sink = SpySink { calls: Mutex::new(vec![]) };
        let report = run_control_loop(
            &goal, &input, &[], &FixedPlanner(a_network(vec![])), &PassthroughSoulGate, &NeverMitigates, &sink,
        ).await.unwrap();

        assert!(!report.replanned, "no failed task -> no replan triggered");
        assert_eq!(report.rag, Rag::Red);
        assert!(report.escalation.is_some());
    }

    #[tokio::test]
    async fn no_failures_and_a_feasible_schedule_never_escalates() {
        let input = schedule_input(vec![a_task("a")]);
        let goal = a_goal();
        let sink = SpySink { calls: Mutex::new(vec![]) };
        let report = run_control_loop(
            &goal, &input, &[], &FixedPlanner(a_network(vec![])), &PassthroughSoulGate, &NeverMitigates, &sink,
        ).await.unwrap();

        assert_eq!(report.rag, Rag::Green);
        assert!(report.escalation.is_none());
        assert!(sink.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_unknown_failed_task_id_errors() {
        let input = schedule_input(vec![a_task("a")]);
        let goal = a_goal();
        let sink = SpySink { calls: Mutex::new(vec![]) };
        let result = run_control_loop(
            &goal, &input, &["no-such-task".to_string()], &FixedPlanner(a_network(vec![])), &PassthroughSoulGate,
            &NeverMitigates, &sink,
        ).await;
        assert!(result.is_err());
    }
}
