---
id: FEAT-073
type: feature
priority: high
complexity: M
estimate_tokens: 60k-100k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-020
depends_on: FEAT-071
---

# FEAT-073 — Daemon loop extraction + full dispatch coverage

## Description
**As** the project
**I want** the daemon's loop bodies and every dispatch arm tested
**So that** `regind`'s logic (minus pure process glue) is fully covered by unit
tests.

## Implementation
- Extract the **scheduler** and **reflection** loop *bodies* into testable tick
  functions (`run_due_schedules(state, now)`, `reflection_tick(state)`); the outer
  `loop {}` becomes a thin caller.
- Extend the existing `dispatch_tests` (generic-writer + in-memory DB + `FakeLlm`
  from FEAT-071) to cover the **remaining arms**: chat / task exec, persona, bus,
  meeting, plan, foreman, deputy, skill-package, context, memory-reflect, and the
  ITIL/desired/metrics/etc. arms not yet covered.
- Cover the bad-request / error branches in `handle_connection`'s request parsing at
  the unit level where possible.

## Acceptance Criteria
1. Scheduler/reflection tick fns are unit-tested (due vs not-due, success/failure,
   fail-safe).
2. Every `dispatch` arm has at least one test (happy path; error path where it has
   one), using `FakeLlm` for LLM-dependent arms.
3. `regind` non-glue line coverage approaches 100% (the remaining glue is covered by
   FEAT-074).
