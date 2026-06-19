---
id: FEAT-068
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-029
---

# FEAT-068 — Soul gate for intent

## Description
**As** the system owner
**I want** the soul to vet what regin intends and does
**So that** autonomous planning stays within regin's values — without per-task human
approval.

## Implementation
- Route through the **soul gate** (identity plane, FEAT-029) three checkpoints:
  **goals** (does this intent fit our values?), **plans** (is this plan acceptable?),
  and **significant actions** at execution time (FEAT-065).
- **Significance is declared per action**; trivial reversible actions skip the soul.
  The **red-lines (FEAT-038) remain the orthogonal hard floor** on every action,
  gated independently.
- A soul rejection returns a reason and blocks activation/execution; the only place a
  human enters the autonomous flow (distinct from feasibility escalation, FEAT-069).

## Acceptance Criteria
1. Goal creation and plan activation are blocked when the soul rejects, with the
   reason recorded.
2. A significant action is soul-checked at execution; a trivial one is not; red-lines
   apply regardless.
3. Soul integration is injectable for tests (fake soul accept/reject); unit-tested.
