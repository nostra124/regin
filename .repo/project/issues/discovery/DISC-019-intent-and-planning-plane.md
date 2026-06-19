---
id: DISC-019
type: discovery
priority: high
status: decided
complexity: XL
spawned_features: FEAT-060..069
---

# DISC-019 — Intent & planning plane (objectives, goals, plans)

## Operating-plane context

A new **intent plane** — the *proactive* complement to the reactive operator loop
(DISC-008..016). Today regin maintains a to-be state and remediates deviations
(reactive). The intent plane lets regin **pursue** outcomes: hold standing
**objectives** and drive toward dated **goals** by **planning** work into a
resource-scheduled task network and executing it.

Distinct from the existing `planning.rs` (FEAT-017, the Mode-A cadence aggregator
that emits priority/capability signals up the org matrix) and from the KPI
"objective" (the CSI cost-vs-reliability function). This plane is regin's own
goal-directed planning.

## Model (resolved with user — guided Q&A 2026-06-19)

### Objectives & goals
- **Objective** = *maintain* a state over time (e.g. uptime > 99%/yr). **Modeled as
  the DISC-008 to-be-state generalized to range over the KPI store + time windows**
  (a temporal/aggregate assertion). An objective breach is a deviation that flows
  through the existing remediation loop. No parallel system.
- **Goal** = *achieve* a target state by a deadline. New first-class entity:
  LLM description + a **target** + deadline; **success criteria are derived at
  planning time — measurable/structural preferred, LLM-judged only where
  measurement is too fuzzy** (the measurable-preferred / LLM-fallback rule, applied
  consistently to task output quality too). Lifecycle: proposed → active →
  achieved / failed / abandoned.
- **Both carry a `priority`** (to arbitrate conflicts) and a **`source`/owner** —
  one of: a **dvalin-hierarchy LLM**, a **human**, or **regin itself**. The source
  is who escalations route back to.
- **Intent dependency graph**: goals/objectives relate to each other — `supports`
  (achieving X also advances/achieves Y) and `conflicts_with` (X works against Y).
  Conflicts are detected and arbitrated by **priority**, with **mitigation** in
  place where two intents pull apart.
- Objectives are **hard constraints**; goals are **prioritized targets**; both are
  pursued under the **cost budget**. Authorship is human or LLM-proposed and
  **approval-gated** (creating/changing an intent needs approval, like to-be-state
  edits).

### Plans, tasks & scheduling
- A **plan** decomposes a goal into a **task network**. A **task is a process node**
  (not an ITIL artifact). Core data per task: **estimated process time**, **required
  inputs**, **planned outputs + quality criteria**; **dependencies** (task→task *and*
  event→task); **temporal attributes** (planned / earliest / latest start, due date,
  deadline).
- **Scheduler = resource-constrained (RCPSP)**: CPM **forward/backward pass**
  (earliest/latest start-finish, slack, critical path, deadline feasibility) **plus
  resources** — **cost budget, execution concurrency, and task-declarable named
  resources** (e.g. a maintenance window, exclusive service access). RAG (below) is
  computed deterministically from the schedule, LLM only for fuzzy goals.
- **Task executor = polymorphic action**: a task's work may be a **skill
  invocation**, an **LLM sub-agent** given inputs + output spec, or a **concrete
  guarded op** (bash/file via the FEAT-038 red-line guardrail) — chosen per task.
  Outputs are verified against the task's **quality criteria** (measurable-preferred,
  LLM fallback).

### Authorization
- **The soul gates goals, plans, and individually significant actions** (values /
  acceptability). Significance is declared per action; trivial reversible actions
  skip the soul. The **red-lines (FEAT-038) remain the orthogonal hard floor on
  every action.** *No per-task human approval* — execution is autonomous; a human is
  only pulled in when the soul rejects, or via escalation (below). **Depends on the
  identity/soul plane (0.6.0).**

### Health, failure & escalation
- Each goal/objective carries a **RAG health**: 🟢 on track · 🟡 off-track but
  mitigations are in place and the goal is **not** endangered · 🔴 off the plan and
  the goal **is** endangered.
- **Task failure is a planning-domain loop** (not an ITIL incident): **mitigate**
  (retry / alternative path) → **replan** (regenerate the schedule from current
  state). RAG transitions track the result.
- **On 🔴, escalate to the goal/objective's source** (dvalin LLM over the bus / human
  via login-greeting or critical push — reusing the DISC-010 channels) with three
  remedies: **provide resources · adjust the goal/objective · replan.** Escalation is
  *not* the soul's job — the soul judges acceptability, the source owns feasibility.

### Events
- A small **internal typed event bus**, **linkable to external events** (e.g. dvalin
  over the FEAT-010 message bus). **Triggers** bind events → tasks/plans
  (`incident.created` can start a plan; `objective.breached`, `goal.created`,
  `schedule.tick`, `task.completed`, …). This is the bridge between the reactive and
  proactive planes.

## Decision

Build the intent plane as **MILESTONE-0.7.0**, depending on the soul (0.6.0) and
building on the operator plane (0.5.0: to-be-state, KPIs, guardrail, escalation,
scheduler). Features spawned below.

## Spawned features (MILESTONE-0.7.0)

- **FEAT-060 — Objective model**: to-be-state generalized over the KPI store + time
  windows (temporal/aggregate assertions); priority, source, RAG; breaches feed the
  existing remediation loop.
- **FEAT-061 — Goal model + store**: target + deadline + planning-time-derived
  success criteria (measurable-preferred / LLM-fallback); lifecycle; priority;
  source; RAG; done-detection.
- **FEAT-062 — Intent dependency & conflict graph**: `supports` / `conflicts_with`
  relations; conflict detection; priority-based arbitration + mitigation.
- **FEAT-063 — Planner (goal → task network)**: LLM decomposition into tasks
  (estimated time, inputs, outputs + quality criteria, task/event deps, temporal
  windows, resource demands); soul-gated.
- **FEAT-064 — RCPSP scheduler**: CPM forward/backward pass + resource constraints
  (cost, concurrency, declarable named resources/windows); slack, critical path,
  deadline & resource feasibility; deterministic RAG computation.
- **FEAT-065 — Task executor (polymorphic action)**: skill / sub-agent / guarded op;
  output verification vs quality criteria; red-lines on every action; soul check on
  significant actions.
- **FEAT-066 — Planning control loop**: mitigate → replan; RAG transitions; on red,
  escalate to the source with the three remedies.
- **FEAT-067 — Event bus + triggers**: internal typed events + external (dvalin)
  ingestion; event→task/plan triggers.
- **FEAT-068 — Soul gate for intent**: acceptability gate over goals, plans, and
  significant actions (depends on identity plane FEAT-029).
- **FEAT-069 — Authorship, prioritization & source-routed escalation**: create/change
  intents (human / dvalin-LLM / regin-self), approval-gated; priority; escalation
  routed to source via DISC-010 channels; RAG surfaced in `regin metrics` + the login
  greeting; CLI verbs.
