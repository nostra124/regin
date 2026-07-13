---
id: MILESTONE-0.7.0
type: milestone
status: planned
depends_on: [MILESTONE-0.6.0]
---

# Milestone 0.7.0 — Intent & planning plane

Turns regin from a **reactive** operator (maintain the to-be state, remediate
deviations — 0.5.0) into a **proactive** one: it holds standing **objectives**,
drives toward dated **goals**, and **plans** the work into a resource-scheduled
task network that it executes autonomously under the soul's judgement.

Derived from **DISC-019**. Builds on the operator plane (0.5.0: to-be-state,
KPIs, guardrail/red-lines, escalation channels, scheduler) and **depends on the
soul / identity plane (0.6.0)** — the soul gates goals, plans, and significant
actions.

## Model (one-paragraph recap)

Objectives = the to-be-state generalized over the KPI store + time windows
(maintain); goals = target + deadline with planning-time-derived success criteria
(achieve). Both carry a priority and a source (dvalin-LLM / human / regin), relate
via a supports/conflicts graph, and are pursued under the cost budget — objectives
as hard constraints, goals as prioritized targets. A plan decomposes a goal into a
task network (each task: estimated time, inputs, outputs + quality criteria,
task/event dependencies, temporal windows, resource demands), scheduled by an RCPSP
engine (CPM forward/backward pass + cost/concurrency/declarable resources). Tasks
execute via polymorphic actions (skill / sub-agent / guarded op), red-line-bounded,
with the soul gating significant actions. Health is RAG per intent; task failure is
a planning-domain mitigate→replan loop; on red, escalate to the intent's source
with three remedies (provide resources / adjust / replan). A small internal event
bus (linkable to dvalin) triggers event→task/plan flows.

## Issues

| ID | Title | From | Status |
|----|-------|------|--------|
| FEAT-060 | Objective model (to-be-state over KPIs + time windows) | DISC-019 | done |
| FEAT-061 | Goal model + store (target/deadline/derived criteria/lifecycle) | DISC-019 | done |
| FEAT-062 | Intent dependency & conflict graph (supports/conflicts) | DISC-019 | done |
| FEAT-063 | Planner: goal → task network | DISC-019 | done |
| FEAT-064 | RCPSP scheduler (CPM + resources, slack/critical path, RAG) | DISC-019 | done |
| FEAT-065 | Task executor (polymorphic action + quality-criteria verify) | DISC-019 | open |
| FEAT-066 | Planning control loop (mitigate→replan→RAG→escalate) | DISC-019 | open |
| FEAT-067 | Event bus + triggers (internal + external/dvalin) | DISC-019 | open |
| FEAT-068 | Soul gate for intent (goals/plans/significant actions) | DISC-019 | open |
| FEAT-069 | Authorship, prioritization & source-routed escalation | DISC-019 | open |

## Suggested delivery order

1. **Model/stores** — FEAT-060 (objectives) · FEAT-061 (goals) · FEAT-062
   (dependency/conflict graph).
2. **Plan & schedule** — FEAT-063 (planner) · FEAT-064 (RCPSP scheduler).
3. **Execute** — FEAT-065 (task executor) · FEAT-068 (soul gate) · FEAT-067 (event
   bus + triggers).
4. **Control** — FEAT-066 (mitigate/replan/RAG/escalate) · FEAT-069 (authorship,
   priority, source-routed escalation + surfacing).

## Exit criteria

- regin holds standing objectives (over KPIs + time) and dated goals, each with a
  priority, a source, and a RAG health; conflicts between intents are detected and
  arbitrated by priority with mitigation.
- A goal is decomposed into a task network and scheduled by the RCPSP engine
  (forward/backward pass, slack, critical path, deadline + resource feasibility);
  the plan and significant actions pass the soul gate; red-lines bound every action.
- Tasks execute via polymorphic actions with output verification against quality
  criteria; failures drive the mitigate→replan loop; on 🔴 the intent's source is
  escalated to with provide-resources / adjust / replan.
- The event bus triggers event→task/plan flows and ingests external (dvalin) events.
- RAG + goal/objective progress surface in `regin metrics` and the login greeting.
- 100% test coverage; no open design questions in any 0.7.0 FEAT (RULE-005).
