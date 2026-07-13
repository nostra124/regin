---
id: FEAT-063
type: feature
priority: high
complexity: L
estimate_tokens: 80k-130k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-061
---

# FEAT-063 — Planner (goal → task network)

## Description
**As** regin
**I want** to decompose a goal into a task network
**So that** there is an executable, schedulable plan toward the goal.

## Implementation
- LLM planner: goal → a **task network**. Each **task** (a process node) carries:
  **estimated process time**, **required inputs**, **planned outputs + quality
  criteria**, **dependencies** (task→task *and* event→task), **temporal attributes**
  (planned / earliest / latest start, due, deadline), and **resource demands** (for
  FEAT-064).
- The plan derives the goal's **measurable success criteria** (feeds FEAT-061).
- The generated plan is **soul-gated** (FEAT-068) before it becomes active.
- Re-entrant: replanning (FEAT-066) regenerates the network from current state.

## Acceptance Criteria
1. A goal is decomposed into tasks with the full task schema (time/inputs/outputs+
   quality/deps/windows/resources); validated as a DAG (no cycles).
2. Event→task dependencies and task→task dependencies are both representable.
3. The plan is submitted to the soul gate before activation; unit-tested with a fake
   planner LLM + fake soul.
