---
id: FEAT-066
type: feature
priority: high
complexity: L
estimate_tokens: 70k-120k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-065
---

# FEAT-066 — Planning control loop (mitigate → replan → escalate)

## Description
**As** regin
**I want** a control loop that keeps plans on track and surfaces trouble honestly
**So that** a failed task doesn't silently sink a goal.

## Implementation
- **RAG health per goal/objective**, computed deterministically from the schedule
  (FEAT-064): 🟢 on track · 🟡 off-track but mitigations in place, not endangered ·
  🔴 off-plan and endangered. (LLM only for fuzzy goals.)
- **Task failure is a planning-domain loop** (never an ITIL incident): **mitigate**
  (retry / alternative path) → **replan** (FEAT-063 regenerates from current state).
  RAG transitions track recovery.
- **On 🔴, escalate to the intent's source** (FEAT-069) with three remedies:
  **provide resources · adjust the goal/objective · replan**.
- Events drive the loop (`task.completed`, `task.failed`, deadline ticks).

## Acceptance Criteria
1. RAG is computed from the schedule across green/yellow/red fixtures.
2. A failed task triggers mitigate→replan; a recovered plan returns to green/yellow,
   not red.
3. An unrecoverable/endangered goal goes red and escalates to its source with the
   three remedies; unit-tested with fakes.
