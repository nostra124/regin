//! RCPSP scheduler: CPM + resources (FEAT-064 / DISC-019).
//!
//! Schedules a `task_network::TaskNetwork`'s tasks: a **CPM forward/backward
//! pass** over task->task precedence (reusing `task_network::validate_dag`
//! rather than re-checking for cycles) yields earliest/latest start/finish,
//! slack, and the critical path; a **resource-constrained serial schedule**
//! then places each task at the earliest time its precedence, temporal
//! window, and resource demands all clear.
//!
//! Two resource categories, both declared on `Task::resource_demands`:
//! - **Renewable** (named resources, e.g. a maintenance window or exclusive
//!   service access, plus the implicit `"concurrency"` slot every task
//!   consumes one unit of): capacity applies to tasks active *simultaneously*
//!   — freed the instant a task finishes.
//! - **Non-renewable** (the reserved `"cost"` key): capacity is the overall
//!   **cost budget** — a cumulative sum across every task in the network,
//!   not a simultaneous-use check.
//!
//! `"concurrency"` and `"cost"` are reserved demand keys; a task declaring
//! `"concurrency"` itself is a configuration error (it's implicit, one unit
//! per active task, from `ScheduleInput::max_concurrency`).
//!
//! Event->task dependencies (`Task::depends_on_events`) don't participate in
//! CPM — same scoping FEAT-063's `validate_dag` already applies; an event is
//! a runtime trigger, not a fixed point in a time-based schedule.
//!
//! `due` and `deadline` are both treated as finish-by caps on a task's
//! latest finish (the tighter of the two applies) — a deliberate
//! simplification over the five-way planned/earliest/latest/due/deadline
//! distinction DISC-019 sketches, since this scheduler computes the
//! "planned" start/finish itself and a due/deadline split in severity
//! doesn't change what's structurally feasible.
//!
//! Pure and deterministic: no I/O, no wall-clock reads (`plan_start` is
//! caller-supplied), ties broken by a stable topological order so the same
//! input always produces the same schedule.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::task_network::{self, Task};

const CONCURRENCY_RESOURCE: &str = "concurrency";
const COST_RESOURCE: &str = "cost";

/// Everything the scheduler needs beyond the task network itself.
pub struct ScheduleInput {
    /// The origin all relative times (and the tasks' own date windows) are
    /// measured from.
    pub plan_start: DateTime<Utc>,
    pub tasks: Vec<Task>,
    /// The execution-concurrency limit: at most this many tasks may be
    /// active at once.
    pub max_concurrency: usize,
    /// Declared capacities for named renewable resources a task may demand
    /// (e.g. `{"maintenance_window": 1.0}`).
    pub resource_capacities: BTreeMap<String, f64>,
    /// The overall cost ceiling — the sum of every task's `"cost"` demand
    /// may not exceed this. `None` means unconstrained.
    pub cost_budget: Option<f64>,
    /// The project-level deadline, if any (e.g. the goal's own deadline).
    pub deadline: Option<DateTime<Utc>>,
}

/// One task's computed CPM times and its actual resource-constrained
/// placement, in minutes relative to `ScheduleInput::plan_start`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSchedule {
    pub task_id: String,
    pub earliest_start: i64,
    pub earliest_finish: i64,
    pub latest_start: i64,
    pub latest_finish: i64,
    pub slack: i64,
    pub scheduled_start: i64,
    pub scheduled_finish: i64,
}

/// The full scheduling result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleReport {
    pub tasks: Vec<TaskSchedule>,
    /// Task ids with zero slack, in execution order.
    pub critical_path: Vec<String>,
    /// `false` iff `issues` is non-empty — a deadline slip, resource
    /// shortfall, or budget overrun was detected.
    pub feasible: bool,
    pub issues: Vec<String>,
}

fn minutes_from(plan_start: DateTime<Utc>, ts: &str) -> Result<i64> {
    let dt = DateTime::parse_from_rfc3339(ts)
        .with_context(|| format!("unparseable timestamp: {ts:?}"))?
        .with_timezone(&Utc);
    Ok((dt - plan_start).num_minutes())
}

/// Deterministic topological order (Kahn's algorithm, ties broken by id) —
/// the DAG-ness itself is already validated by `task_network::validate_dag`.
fn topo_order(tasks: &[Task]) -> Vec<&Task> {
    let by_id: BTreeMap<&str, &Task> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut indegree: BTreeMap<&str, usize> = tasks.iter().map(|t| (t.id.as_str(), 0)).collect();
    let mut successors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in tasks {
        for dep in &t.depends_on_tasks {
            *indegree.get_mut(t.id.as_str()).expect("validated") += 1;
            successors.entry(dep.as_str()).or_default().push(t.id.as_str());
        }
    }

    let mut ready: Vec<&str> = indegree.iter().filter(|(_, d)| **d == 0).map(|(id, _)| *id).collect();
    ready.sort();
    let mut queue: VecDeque<&str> = ready.into();

    let mut order = Vec::with_capacity(tasks.len());
    while let Some(id) = queue.pop_front() {
        order.push(by_id[id]);
        if let Some(succs) = successors.get(id) {
            let mut newly_ready = Vec::new();
            for &s in succs {
                let d = indegree.get_mut(s).expect("validated");
                *d -= 1;
                if *d == 0 {
                    newly_ready.push(s);
                }
            }
            newly_ready.sort();
            for r in newly_ready {
                queue.push_back(r);
            }
        }
    }
    order
}

struct Cpm {
    es: BTreeMap<String, i64>,
    ef: BTreeMap<String, i64>,
    ls: BTreeMap<String, i64>,
    lf: BTreeMap<String, i64>,
}

fn cpm_pass(order: &[&Task], plan_start: DateTime<Utc>, deadline: Option<i64>) -> Result<Cpm> {
    let mut es: BTreeMap<String, i64> = BTreeMap::new();
    let mut ef: BTreeMap<String, i64> = BTreeMap::new();
    for t in order {
        let mut start = 0i64;
        for dep in &t.depends_on_tasks {
            start = start.max(*ef.get(dep).context("dependency scheduled out of order")?);
        }
        if let Some(w) = &t.earliest_start {
            start = start.max(minutes_from(plan_start, w)?);
        }
        let duration = t.estimated_minutes.max(0);
        es.insert(t.id.clone(), start);
        ef.insert(t.id.clone(), start + duration);
    }

    let project_finish = ef.values().copied().max().unwrap_or(0);
    let horizon = deadline.unwrap_or(project_finish);

    let mut successors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in order {
        for dep in &t.depends_on_tasks {
            successors.entry(dep.as_str()).or_default().push(t.id.as_str());
        }
    }

    let mut ls: BTreeMap<String, i64> = BTreeMap::new();
    let mut lf: BTreeMap<String, i64> = BTreeMap::new();
    for t in order.iter().rev() {
        let mut finish_cap = horizon;
        if let Some(succs) = successors.get(t.id.as_str()) {
            let min_succ_ls = succs.iter().map(|s| ls[*s]).min().expect("non-empty");
            finish_cap = finish_cap.min(min_succ_ls);
        }
        if let Some(w) = &t.deadline {
            finish_cap = finish_cap.min(minutes_from(plan_start, w)?);
        }
        if let Some(w) = &t.due {
            finish_cap = finish_cap.min(minutes_from(plan_start, w)?);
        }
        let duration = t.estimated_minutes.max(0);
        let mut start = finish_cap - duration;
        if let Some(w) = &t.latest_start {
            start = start.min(minutes_from(plan_start, w)?);
        }
        ls.insert(t.id.clone(), start);
        lf.insert(t.id.clone(), start + duration);
    }

    Ok(Cpm { es, ef, ls, lf })
}

/// A resource-constrained task placement already committed to the schedule.
struct Placed {
    start: i64,
    finish: i64,
    demands: BTreeMap<String, f64>,
}

fn full_demands(task: &Task) -> BTreeMap<String, f64> {
    let mut d = BTreeMap::new();
    d.insert(CONCURRENCY_RESOURCE.to_string(), 1.0);
    for (k, v) in &task.resource_demands {
        if k != COST_RESOURCE {
            d.insert(k.clone(), *v);
        }
    }
    d
}

fn capacity_of(input: &ScheduleInput, resource: &str) -> f64 {
    if resource == CONCURRENCY_RESOURCE {
        input.max_concurrency as f64
    } else {
        *input.resource_capacities.get(resource).unwrap_or(&0.0)
    }
}

fn usage_at(scheduled: &[Placed], t: i64, resource: &str) -> f64 {
    scheduled
        .iter()
        .filter(|p| p.start <= t && t < p.finish)
        .filter_map(|p| p.demands.get(resource))
        .sum()
}

/// Earliest start >= `lower_bound` at which every demanded resource stays
/// within capacity for the task's whole duration. Errors if a single
/// resource's demand alone exceeds its declared capacity (or an undeclared
/// resource, capacity 0) — no time shift can ever make that fit.
fn earliest_feasible_start(
    scheduled: &[Placed],
    demands: &BTreeMap<String, f64>,
    duration: i64,
    lower_bound: i64,
    input: &ScheduleInput,
) -> Result<i64> {
    for (resource, &need) in demands {
        let cap = capacity_of(input, resource);
        if need > cap + f64::EPSILON {
            bail!("demands {need} of resource {resource:?} but capacity is only {cap}");
        }
    }
    if duration == 0 {
        return Ok(lower_bound);
    }

    let mut candidate = lower_bound;
    for _ in 0..(scheduled.len() + 1) {
        let end = candidate + duration;
        let mut events: BTreeSet<i64> = BTreeSet::from([candidate]);
        for p in scheduled {
            if p.start < end && p.finish > candidate {
                if p.start > candidate && p.start < end {
                    events.insert(p.start);
                }
                if p.finish > candidate && p.finish < end {
                    events.insert(p.finish);
                }
            }
        }

        let mut push_to: Option<i64> = None;
        for &t in &events {
            for (resource, &need) in demands {
                if need <= 0.0 {
                    continue;
                }
                let cap = capacity_of(input, resource);
                let used = usage_at(scheduled, t, resource);
                if used + need > cap + f64::EPSILON {
                    let next = scheduled
                        .iter()
                        .filter(|p| p.start <= t && t < p.finish && p.demands.get(resource).copied().unwrap_or(0.0) > 0.0)
                        .map(|p| p.finish)
                        .min();
                    push_to = Some(push_to.map_or(next.unwrap_or(t + 1), |cur| cur.min(next.unwrap_or(t + 1))));
                }
            }
        }
        match push_to {
            None => return Ok(candidate),
            Some(p) => candidate = p.max(candidate + 1),
        }
    }
    bail!("could not find a feasible placement within the scheduled horizon")
}

/// Schedule a task network: a CPM forward/backward pass (acceptance
/// criterion 1) followed by resource-constrained placement in topological
/// order (acceptance criterion 2). Never errors on infeasibility — a
/// deadline slip, resource shortfall, or budget overrun is reported via
/// `issues`/`feasible` instead (acceptance criterion 3); this function only
/// errors on a structural problem with the input itself (a cycle, a
/// reserved resource name misused, an unparseable date window).
pub fn schedule(input: &ScheduleInput) -> Result<ScheduleReport> {
    task_network::validate_dag(&input.tasks)?;
    for t in &input.tasks {
        if t.resource_demands.contains_key(CONCURRENCY_RESOURCE) {
            bail!("task {:?} declares the reserved resource {CONCURRENCY_RESOURCE:?} explicitly", t.id);
        }
    }

    let order = topo_order(&input.tasks);
    let deadline_minutes = input.deadline.map(|d| (d - input.plan_start).num_minutes());
    let cpm = cpm_pass(&order, input.plan_start, deadline_minutes)?;

    let mut issues = Vec::new();
    let mut feasible = true;
    for t in &order {
        if cpm.ls[&t.id] < cpm.es[&t.id] {
            issues.push(format!("task {:?} has negative slack ({} min) — the deadline cannot be met", t.id, cpm.ls[&t.id] - cpm.es[&t.id]));
            feasible = false;
        }
    }

    if let Some(budget) = input.cost_budget {
        let total_cost: f64 = input.tasks.iter().filter_map(|t| t.resource_demands.get(COST_RESOURCE)).sum();
        if total_cost > budget + f64::EPSILON {
            issues.push(format!("total cost {total_cost} exceeds the budget of {budget}"));
            feasible = false;
        }
    }

    let mut placed: Vec<Placed> = Vec::with_capacity(order.len());
    let mut scheduled_finish: BTreeMap<String, i64> = BTreeMap::new();
    let mut scheduled_start: BTreeMap<String, i64> = BTreeMap::new();
    for t in &order {
        let mut lower_bound = cpm.es[&t.id];
        for dep in &t.depends_on_tasks {
            lower_bound = lower_bound.max(scheduled_finish[dep]);
        }
        let duration = t.estimated_minutes.max(0);
        let demands = full_demands(t);
        match earliest_feasible_start(&placed, &demands, duration, lower_bound, input) {
            Ok(start) => {
                let finish = start + duration;
                scheduled_start.insert(t.id.clone(), start);
                scheduled_finish.insert(t.id.clone(), finish);
                placed.push(Placed { start, finish, demands });
            }
            Err(e) => {
                issues.push(format!("task {:?} could not be resource-scheduled: {e}", t.id));
                feasible = false;
                scheduled_start.insert(t.id.clone(), lower_bound);
                scheduled_finish.insert(t.id.clone(), lower_bound + duration);
            }
        }
    }

    if let Some(dl) = deadline_minutes {
        let overall_finish = scheduled_finish.values().copied().max().unwrap_or(0);
        if overall_finish > dl {
            issues.push(format!("the resource-constrained schedule finishes at {overall_finish} min, past the deadline of {dl} min"));
            feasible = false;
        }
    }

    let critical_path: Vec<String> = order.iter().filter(|t| cpm.ls[&t.id] == cpm.es[&t.id]).map(|t| t.id.clone()).collect();

    let tasks = order
        .iter()
        .map(|t| TaskSchedule {
            task_id: t.id.clone(),
            earliest_start: cpm.es[&t.id],
            earliest_finish: cpm.ef[&t.id],
            latest_start: cpm.ls[&t.id],
            latest_finish: cpm.lf[&t.id],
            slack: cpm.ls[&t.id] - cpm.es[&t.id],
            scheduled_start: scheduled_start[&t.id],
            scheduled_finish: scheduled_finish[&t.id],
        })
        .collect();

    Ok(ScheduleReport { tasks, critical_path, feasible, issues })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, minutes: i64, deps: &[&str]) -> Task {
        Task {
            id: id.into(),
            title: id.into(),
            estimated_minutes: minutes,
            inputs: vec![],
            outputs: vec![],
            quality_criteria: vec![],
            depends_on_tasks: deps.iter().map(|s| s.to_string()).collect(),
            depends_on_events: vec![],
            earliest_start: None,
            latest_start: None,
            due: None,
            deadline: None,
            resource_demands: BTreeMap::new(),
        }
    }

    fn plan_start() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().with_timezone(&Utc)
    }

    fn plus_minutes(minutes: i64) -> String {
        (plan_start() + chrono::Duration::minutes(minutes)).to_rfc3339()
    }

    fn base_input(tasks: Vec<Task>) -> ScheduleInput {
        ScheduleInput {
            plan_start: plan_start(),
            tasks,
            max_concurrency: 100,
            resource_capacities: BTreeMap::new(),
            cost_budget: None,
            deadline: None,
        }
    }

    // --- acceptance criterion 1: CPM forward/backward pass -----------------

    #[test]
    fn diamond_network_yields_correct_times_slack_and_critical_path() {
        // a(10) -> b(20) -> d(5); a(10) -> c(5) -> d(5). b is the long branch.
        let tasks = vec![task("a", 10, &[]), task("b", 20, &["a"]), task("c", 5, &["a"]), task("d", 5, &["b", "c"])];
        let report = schedule(&base_input(tasks)).unwrap();
        let by_id: BTreeMap<&str, &TaskSchedule> = report.tasks.iter().map(|t| (t.task_id.as_str(), t)).collect();

        assert_eq!(by_id["a"].earliest_start, 0);
        assert_eq!(by_id["a"].earliest_finish, 10);
        assert_eq!(by_id["b"].earliest_start, 10);
        assert_eq!(by_id["b"].earliest_finish, 30);
        assert_eq!(by_id["c"].earliest_start, 10);
        assert_eq!(by_id["c"].earliest_finish, 15);
        assert_eq!(by_id["d"].earliest_start, 30);
        assert_eq!(by_id["d"].earliest_finish, 35);

        // c has slack: it could start as late as d's latest start (30) minus its own duration (5) = 25,
        // vs its earliest start of 10 -> 15 min of slack.
        assert_eq!(by_id["c"].slack, 15);
        assert_eq!(by_id["a"].slack, 0);
        assert_eq!(by_id["b"].slack, 0);
        assert_eq!(by_id["d"].slack, 0);

        assert_eq!(report.critical_path, vec!["a", "b", "d"]);
        assert!(report.feasible);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn a_single_task_has_zero_slack_and_is_its_own_critical_path() {
        let report = schedule(&base_input(vec![task("a", 15, &[])])).unwrap();
        assert_eq!(report.tasks[0].slack, 0);
        assert_eq!(report.critical_path, vec!["a"]);
    }

    #[test]
    fn earliest_start_window_pushes_out_the_forward_pass() {
        let mut a = task("a", 10, &[]);
        a.earliest_start = Some(plus_minutes(50));
        let report = schedule(&base_input(vec![a])).unwrap();
        assert_eq!(report.tasks[0].earliest_start, 50);
        assert_eq!(report.tasks[0].earliest_finish, 60);
    }

    #[test]
    fn deadline_and_latest_start_cap_the_backward_pass() {
        let mut a = task("a", 10, &[]);
        a.latest_start = Some(plus_minutes(5)); // tighter than the deadline would otherwise allow
        let mut input = base_input(vec![a]);
        input.deadline = Some(plan_start() + chrono::Duration::minutes(100));
        let report = schedule(&input).unwrap();
        assert_eq!(report.tasks[0].latest_start, 5);
        assert_eq!(report.tasks[0].latest_finish, 15);
    }

    // --- acceptance criterion 2: resource capacities honoured --------------

    #[test]
    fn concurrency_limit_serializes_otherwise_parallel_tasks() {
        let tasks = vec![task("a", 10, &[]), task("b", 10, &[])];
        let mut input = base_input(tasks);
        input.max_concurrency = 1;
        let report = schedule(&input).unwrap();
        let by_id: BTreeMap<&str, &TaskSchedule> = report.tasks.iter().map(|t| (t.task_id.as_str(), t)).collect();
        // both want to start at 0 (no precedence between them); concurrency=1 forces one to wait.
        let starts: BTreeSet<i64> = [by_id["a"].scheduled_start, by_id["b"].scheduled_start].into();
        assert_eq!(starts, BTreeSet::from([0, 10]));
        assert!(report.feasible);
    }

    #[test]
    fn a_named_resource_capacity_defers_a_conflicting_task() {
        let mut a = task("a", 10, &[]);
        a.resource_demands.insert("bay".into(), 1.0);
        let mut b = task("b", 10, &[]);
        b.resource_demands.insert("bay".into(), 1.0);
        let mut input = base_input(vec![a, b]);
        input.resource_capacities.insert("bay".into(), 1.0);
        let report = schedule(&input).unwrap();
        let by_id: BTreeMap<&str, &TaskSchedule> = report.tasks.iter().map(|t| (t.task_id.as_str(), t)).collect();
        let starts: BTreeSet<i64> = [by_id["a"].scheduled_start, by_id["b"].scheduled_start].into();
        assert_eq!(starts, BTreeSet::from([0, 10]), "exclusive bay serializes the two tasks");
        assert!(report.feasible);
    }

    #[test]
    fn a_named_resource_with_enough_capacity_allows_full_parallelism() {
        let mut a = task("a", 10, &[]);
        a.resource_demands.insert("bay".into(), 1.0);
        let mut b = task("b", 10, &[]);
        b.resource_demands.insert("bay".into(), 1.0);
        let mut input = base_input(vec![a, b]);
        input.resource_capacities.insert("bay".into(), 2.0);
        let report = schedule(&input).unwrap();
        for t in &report.tasks {
            assert_eq!(t.scheduled_start, 0);
        }
        assert!(report.feasible);
    }

    #[test]
    fn a_task_demanding_more_than_the_total_capacity_is_reported_infeasible() {
        let mut a = task("a", 10, &[]);
        a.resource_demands.insert("bay".into(), 5.0);
        let mut input = base_input(vec![a]);
        input.resource_capacities.insert("bay".into(), 1.0);
        let report = schedule(&input).unwrap();
        assert!(!report.feasible);
        assert!(report.issues.iter().any(|i| i.contains("bay")));
    }

    #[test]
    fn declaring_the_reserved_concurrency_resource_directly_is_an_error() {
        let mut a = task("a", 10, &[]);
        a.resource_demands.insert(CONCURRENCY_RESOURCE.into(), 1.0);
        assert!(schedule(&base_input(vec![a])).is_err());
    }

    // --- acceptance criterion 3: infeasibility reporting --------------------

    #[test]
    fn a_deadline_tighter_than_the_critical_path_is_reported_infeasible() {
        let tasks = vec![task("a", 100, &[])];
        let mut input = base_input(tasks);
        input.deadline = Some(plan_start() + chrono::Duration::minutes(10));
        let report = schedule(&input).unwrap();
        assert!(!report.feasible);
        assert!(report.issues.iter().any(|i| i.contains("negative slack")));
    }

    #[test]
    fn a_feasible_deadline_reports_no_issues() {
        let tasks = vec![task("a", 10, &[])];
        let mut input = base_input(tasks);
        input.deadline = Some(plan_start() + chrono::Duration::minutes(100));
        let report = schedule(&input).unwrap();
        assert!(report.feasible);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn an_over_budget_plan_is_reported_infeasible() {
        let mut a = task("a", 10, &[]);
        a.resource_demands.insert(COST_RESOURCE.into(), 30.0);
        let mut b = task("b", 10, &[]);
        b.resource_demands.insert(COST_RESOURCE.into(), 30.0);
        let mut input = base_input(vec![a, b]);
        input.cost_budget = Some(50.0);
        let report = schedule(&input).unwrap();
        assert!(!report.feasible);
        assert!(report.issues.iter().any(|i| i.contains("budget")));
    }

    #[test]
    fn a_plan_within_budget_reports_no_cost_issue() {
        let mut a = task("a", 10, &[]);
        a.resource_demands.insert(COST_RESOURCE.into(), 20.0);
        let mut input = base_input(vec![a]);
        input.cost_budget = Some(50.0);
        let report = schedule(&input).unwrap();
        assert!(report.feasible);
    }

    #[test]
    fn schedule_rejects_a_cyclic_network() {
        let tasks = vec![task("a", 10, &["b"]), task("b", 10, &["a"])];
        assert!(schedule(&base_input(tasks)).is_err());
    }

    #[test]
    fn resource_and_deadline_infeasibility_can_both_be_reported_together() {
        let mut a = task("a", 10, &[]);
        a.resource_demands.insert("bay".into(), 5.0);
        let mut input = base_input(vec![a]);
        input.resource_capacities.insert("bay".into(), 1.0);
        input.deadline = Some(plan_start() + chrono::Duration::minutes(5));
        let report = schedule(&input).unwrap();
        assert!(!report.feasible);
        assert!(!report.issues.is_empty());
    }
}
