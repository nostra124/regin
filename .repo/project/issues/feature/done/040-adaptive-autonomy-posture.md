---
id: FEAT-040
type: feature
priority: medium
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-009
depends_on: FEAT-050
---

# FEAT-040 — Adaptive autonomy posture

## Description
**As** the system owner
**I want** regin to start conservative and earn more autonomy as it proves itself
**So that** auto-apply expands on evidence, not optimism.

## Implementation
- Default posture is **conservative**: most fixes route to `pending_approval`.
- The safe-lane auto-apply set **graduates** as change-success-rate / autonomy KPIs
  (FEAT-050) prove trust — the same earn-trust-with-evidence pattern as DISC-015's
  promotion loop, governed by the same KPI store.
- Posture is **tunable** (a setting bounding how much may auto-apply).
- Graduation is reversible: a rise in change-failure / promotion-error demotes the
  posture.

## Acceptance Criteria
1. Out of the box, reversible fixes still default to approval until KPIs cross the
   trust threshold.
2. Sustained change-success graduates specific safe-lane ops to auto-apply; a failure
   spike demotes them.
3. The posture ceiling is tunable and honoured; unit-tested with seeded KPIs.
