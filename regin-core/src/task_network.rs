//! Planner: goal -> task network (FEAT-063 / DISC-019).
//!
//! Decomposes a [`goal::Goal`] into an executable, schedulable
//! [`TaskNetwork`]: each [`Task`] (a process node) carries estimated time,
//! inputs/outputs + quality criteria, task->task *and* event->task
//! dependencies, temporal attributes, and resource demands (consumed by
//! FEAT-064's RCPSP scheduler). The plan also derives the goal's measurable
//! success criteria (feeds `goal::SuccessCriterion`, FEAT-061).
//!
//! Like [`crate::decision`] and [`crate::remediation`], this is a **pure-ish
//! engine** — no database here; [`TaskPlanner`] is injectable so tests never
//! need a real LLM (mirrors `decision::Planner`).
//!
//! The generated network is gated by the Soul **before** it becomes active
//! — reusing `decision::SoulGate`/`SoulVerdict` directly rather than growing
//! a parallel gating mechanism (the same "no parallel evaluator" principle
//! FEAT-060 established for observed-vs-target checks). Only
//! [`decision::Plan::intent_summary`] is ever sent to the real
//! (`LlmSoulGate`) implementation, so [`plan_and_gate`] summarizes the
//! network into one string rather than exposing task detail to the vote.
//! FEAT-068 ("soul gate for intent") is the *policy* layer on top — which
//! goals/plans get gated and how escalation routes — not a second gate
//! mechanism.
//!
//! Re-entrant by construction: [`TaskPlanner::plan`] takes optional revision
//! feedback, the same shape `decision::Planner::plan` uses, so a future
//! replanning loop (FEAT-066) can regenerate the network from current state
//! without a new trait.

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::decision::{Plan as SoulPlan, SoulEvaluation, SoulGate, SoulVerdict};
use crate::goal::{Goal, SuccessCriterion};

/// One process node in a task network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub estimated_minutes: i64,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Measurable-preferred, LLM-judged fallback — the same shape a goal's
    /// own success criteria use ([`SuccessCriterion`]); FEAT-065's executor
    /// verifies a task's output against these.
    pub quality_criteria: Vec<SuccessCriterion>,
    /// task -> task dependencies, by [`Task::id`].
    pub depends_on_tasks: Vec<String>,
    /// event -> task dependencies, by event name (FEAT-067's event bus).
    pub depends_on_events: Vec<String>,
    pub earliest_start: Option<String>,
    pub latest_start: Option<String>,
    pub due: Option<String>,
    pub deadline: Option<String>,
    /// resource name -> demanded amount (FEAT-064's RCPSP scheduler).
    pub resource_demands: BTreeMap<String, f64>,
}

/// A goal decomposed into an executable, schedulable network of tasks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskNetwork {
    pub id: String,
    pub goal_id: String,
    pub tasks: Vec<Task>,
    /// Success criteria this plan implies — ready to attach to the goal
    /// (feeds `goal::SuccessCriterion`, FEAT-061).
    pub derived_criteria: Vec<SuccessCriterion>,
}

/// Validates a task network's task->task dependencies form a DAG: every
/// referenced id must exist, and no cycle (acceptance criterion 1).
/// event->task dependencies (`Task::depends_on_events`) are excluded from
/// cycle checking by construction — an event name never resolves to a task
/// id, so it can never participate in a cycle.
pub fn validate_dag(tasks: &[Task]) -> Result<()> {
    let ids: BTreeSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    for t in tasks {
        for dep in &t.depends_on_tasks {
            if !ids.contains(dep.as_str()) {
                bail!("task {:?} depends on unknown task {dep:?}", t.id);
            }
        }
    }

    let by_id: BTreeMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut done: BTreeSet<&str> = BTreeSet::new();

    fn visit<'a>(
        id: &'a str,
        by_id: &BTreeMap<&'a str, &'a Task>,
        done: &mut BTreeSet<&'a str>,
        stack: &mut Vec<&'a str>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if stack.contains(&id) {
            let mut path = stack.clone();
            path.push(id);
            bail!("cycle in task network: {}", path.join(" -> "));
        }
        stack.push(id);
        if let Some(t) = by_id.get(id) {
            for dep in &t.depends_on_tasks {
                visit(dep.as_str(), by_id, done, stack)?;
            }
        }
        stack.pop();
        done.insert(id);
        Ok(())
    }

    for t in tasks {
        let mut stack = Vec::new();
        visit(t.id.as_str(), &by_id, &mut done, &mut stack)?;
    }
    Ok(())
}

/// Produces a [`TaskNetwork`] for a goal, with no side effects — the
/// production implementation asks an LLM; tests supply a fake. Takes
/// optional revision feedback so a replanning loop (FEAT-066) can call it
/// again from current state without a different trait (mirrors
/// `decision::Planner::plan`'s shape).
#[async_trait]
pub trait TaskPlanner: Send + Sync {
    async fn plan(&self, goal: &Goal, revision_feedback: Option<&str>) -> Result<TaskNetwork>;
}

/// A planned network paired with the Soul's verdict on it.
#[derive(Debug, Clone)]
pub struct PlannedNetwork {
    pub network: TaskNetwork,
    pub soul: SoulEvaluation,
}

impl PlannedNetwork {
    /// Whether the network cleared the gate and may become active.
    pub fn approved(&self) -> bool {
        self.soul.verdict == SoulVerdict::Approve
    }
}

/// Plan a goal into a task network, validate it as a DAG, and submit it to
/// the Soul gate before it may become active (acceptance criterion 3). An
/// invalid DAG errors before ever reaching the Soul — a network that can't
/// execute isn't worth a vote. `revision_feedback` is threaded straight to
/// [`TaskPlanner::plan`] — `None` for a first pass, `Some(...)` when
/// FEAT-066's control loop calls this again to replan from a task failure.
pub async fn plan_and_gate(
    goal: &Goal,
    revision_feedback: Option<&str>,
    planner: &dyn TaskPlanner,
    soul: &dyn SoulGate,
) -> Result<PlannedNetwork> {
    let network = planner.plan(goal, revision_feedback).await?;
    validate_dag(&network.tasks)?;

    let steps: Vec<String> = network.tasks.iter().map(|t| t.title.clone()).collect();
    let intent_summary = format!(
        "Plan toward goal {:?} (target: {:?}): {} task(s) — {}",
        goal.description,
        goal.target,
        network.tasks.len(),
        steps.join("; "),
    );
    let soul_plan = SoulPlan {
        id: network.id.clone(),
        intent_summary,
        steps,
        intended_tool_calls: Vec::new(),
    };
    let evaluation = soul.evaluate(&soul_plan).await?;
    Ok(PlannedNetwork { network, soul: evaluation })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{PassthroughSoulGate, RawSoulVerdict};
    use crate::desired::{AssertOp, AssertValue};
    use std::sync::Mutex;

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

    fn a_task(id: &str, depends_on_tasks: &[&str]) -> Task {
        Task {
            id: id.into(),
            title: format!("task {id}"),
            estimated_minutes: 30,
            inputs: vec![],
            outputs: vec![],
            quality_criteria: vec![],
            depends_on_tasks: depends_on_tasks.iter().map(|s| s.to_string()).collect(),
            depends_on_events: vec![],
            earliest_start: None,
            latest_start: None,
            due: None,
            deadline: None,
            resource_demands: BTreeMap::new(),
        }
    }

    struct FakePlanner(TaskNetwork);

    #[async_trait]
    impl TaskPlanner for FakePlanner {
        async fn plan(&self, _goal: &Goal, _revision_feedback: Option<&str>) -> Result<TaskNetwork> {
            Ok(self.0.clone())
        }
    }

    struct SpyPlanner {
        network: TaskNetwork,
        calls: Mutex<Vec<(String, Option<String>)>>,
    }

    #[async_trait]
    impl TaskPlanner for SpyPlanner {
        async fn plan(&self, goal: &Goal, revision_feedback: Option<&str>) -> Result<TaskNetwork> {
            self.calls.lock().unwrap().push((goal.id.clone(), revision_feedback.map(|s| s.to_string())));
            Ok(self.network.clone())
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

    #[test]
    fn validate_dag_accepts_a_simple_chain() {
        let tasks = vec![a_task("a", &[]), a_task("b", &["a"]), a_task("c", &["b"])];
        assert!(validate_dag(&tasks).is_ok());
    }

    #[test]
    fn validate_dag_rejects_an_unknown_dependency() {
        let tasks = vec![a_task("a", &["no-such-task"])];
        assert!(validate_dag(&tasks).is_err());
    }

    #[test]
    fn validate_dag_rejects_a_self_cycle() {
        let tasks = vec![a_task("a", &["a"])];
        assert!(validate_dag(&tasks).is_err());
    }

    #[test]
    fn validate_dag_rejects_a_multi_node_cycle() {
        let tasks = vec![a_task("a", &["c"]), a_task("b", &["a"]), a_task("c", &["b"])];
        assert!(validate_dag(&tasks).is_err());
    }

    #[test]
    fn event_and_task_dependencies_are_both_representable() {
        // acceptance criterion 2
        let mut t = a_task("b", &["a"]);
        t.depends_on_events.push("disk.threshold_breached".into());
        let tasks = vec![a_task("a", &[]), t];
        assert!(validate_dag(&tasks).is_ok());
        assert_eq!(tasks[1].depends_on_tasks, vec!["a".to_string()]);
        assert_eq!(tasks[1].depends_on_events, vec!["disk.threshold_breached".to_string()]);
    }

    fn a_network(tasks: Vec<Task>) -> TaskNetwork {
        TaskNetwork {
            id: "net-1".into(),
            goal_id: "goal-1".into(),
            tasks,
            derived_criteria: vec![SuccessCriterion::Measurable {
                key: "disk.root.use_percent".into(),
                op: AssertOp::Lt,
                value: AssertValue::Num(80.0),
                description: Some("root stays under 80%".into()),
            }],
        }
    }

    #[tokio::test]
    async fn plan_and_gate_approves_a_valid_network_through_a_passthrough_soul() {
        // acceptance criterion 3
        let network = a_network(vec![a_task("a", &[])]);
        let planner = FakePlanner(network.clone());
        let planned = plan_and_gate(&a_goal(), None, &planner, &PassthroughSoulGate).await.unwrap();
        assert!(planned.approved());
        assert_eq!(planned.network, network);
    }

    #[tokio::test]
    async fn plan_and_gate_reports_not_approved_when_the_soul_vetoes() {
        let network = a_network(vec![a_task("a", &[])]);
        let planner = FakePlanner(network);
        let planned = plan_and_gate(&a_goal(), None, &planner, &AlwaysVetoSoul).await.unwrap();
        assert!(!planned.approved());
        assert_eq!(planned.soul.verdict, SoulVerdict::Veto);
    }

    #[tokio::test]
    async fn plan_and_gate_rejects_a_cyclic_network_before_reaching_the_soul() {
        let network = a_network(vec![a_task("a", &["a"])]);
        let planner = FakePlanner(network);
        assert!(plan_and_gate(&a_goal(), None, &planner, &AlwaysVetoSoul).await.is_err());
    }

    #[tokio::test]
    async fn plan_and_gate_forwards_the_goal_and_no_feedback_on_a_first_pass() {
        let network = a_network(vec![a_task("a", &[])]);
        let goal = a_goal();
        let spy = SpyPlanner { network, calls: Mutex::new(vec![]) };
        plan_and_gate(&goal, None, &spy, &PassthroughSoulGate).await.unwrap();
        assert_eq!(spy.calls.lock().unwrap().as_slice(), &[(goal.id.clone(), None)]);
    }

    #[tokio::test]
    async fn plan_and_gate_forwards_revision_feedback_on_a_replan() {
        let network = a_network(vec![a_task("a", &[])]);
        let goal = a_goal();
        let spy = SpyPlanner { network, calls: Mutex::new(vec![]) };
        plan_and_gate(&goal, Some("task a failed"), &spy, &PassthroughSoulGate).await.unwrap();
        assert_eq!(spy.calls.lock().unwrap().as_slice(), &[(goal.id.clone(), Some("task a failed".to_string()))]);
    }

    #[tokio::test]
    async fn derived_criteria_are_carried_through_from_the_planner() {
        let network = a_network(vec![a_task("a", &[])]);
        let expected = network.derived_criteria.clone();
        let planner = FakePlanner(network);
        let planned = plan_and_gate(&a_goal(), None, &planner, &PassthroughSoulGate).await.unwrap();
        assert_eq!(planned.network.derived_criteria, expected);
        assert!(!planned.network.derived_criteria.is_empty());
    }
}
