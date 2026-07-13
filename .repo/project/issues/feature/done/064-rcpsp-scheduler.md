---
id: FEAT-064
type: feature
priority: high
complexity: L
estimate_tokens: 90k-140k
estimate_time: 150-210min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-063
---

# FEAT-064 — RCPSP scheduler (CPM + resources)

## Description
**As** regin
**I want** a resource-constrained schedule for a task network
**So that** I know when tasks run, what's critical, and whether the deadline is
feasible.

## Implementation
- **CPM forward/backward pass**: earliest/latest start-finish, **slack**, **critical
  path**, from task durations + dependencies + date windows (planned/earliest/latest
  start, due, deadline).
- **Resource constraints (RCPSP)**: schedule respects the **cost budget**, an
  **execution-concurrency** limit, and **task-declarable named resources** (e.g. a
  maintenance window, exclusive service access) with capacities.
- **Feasibility**: report deadline feasibility and resource-shortfall; this drives the
  RAG health (FEAT-066) deterministically (LLM only for fuzzy goals).
- Pure, deterministic, heavily unit-tested (the scheduling core).

## Acceptance Criteria
1. Forward/backward pass yields correct earliest/latest times, slack, and the
   critical path on known fixtures.
2. Resource capacities are honoured (no over-allocation of concurrency / a named
   resource / budget); a window constraint defers a task correctly.
3. An infeasible deadline or resource shortfall is reported; unit-tested across
   feasible and infeasible fixtures.
