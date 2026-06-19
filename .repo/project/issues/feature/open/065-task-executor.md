---
id: FEAT-065
type: feature
priority: high
complexity: L
estimate_tokens: 80k-130k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-064
---

# FEAT-065 — Task executor (polymorphic action)

## Description
**As** regin
**I want** to execute a ready task and verify its outputs
**So that** plans actually get done, safely and to spec.

## Implementation
- Execute a schedule-ready task via a **polymorphic action**: a **skill invocation**
  (FEAT-045), an **LLM sub-agent** given the task's inputs + output spec, or a
  **concrete guarded op** (bash/file) — chosen per task.
- **Every action is red-line-bounded** (FEAT-038 guardrail); **significant actions**
  additionally pass the **soul gate** (FEAT-068). Trivial reversible actions skip the
  soul.
- **Output verification** against the task's **quality criteria**
  (measurable-preferred, LLM fallback). Pass → task complete (emit `task.completed`);
  fail → task failed → planning control loop (FEAT-066).
- Respects concurrency from the scheduler (FEAT-064).

## Acceptance Criteria
1. Each action kind (skill / sub-agent / guarded op) executes and its outputs are
   verified against quality criteria.
2. A red-line action is refused; a significant action consults the soul; a trivial
   one does not (unit-tested with fakes).
3. Output failing quality criteria marks the task failed and hands off to the control
   loop; success emits `task.completed`.
