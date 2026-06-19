---
id: FEAT-043
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-010
depends_on: FEAT-041
---

# FEAT-043 — Standalone parking + login greeting

## Description
**As** an operator running regin standalone
**I want** items needing me parked locally and shown when I next log in
**So that** there's a pull-at-login channel when no supervisor bus is reachable.

## Implementation
- When effective mode is **standalone** (FEAT-041), **park** decision/approval items
  locally instead of pushing.
- `regin chat` opens with a **greeting**: a one-line health summary (per-domain
  to-be-state vs reality, FEAT-034; counts open incidents) + the **actionable items**
  (changes awaiting approval, problems needing a decision). Approve/decide inline.
- **Auto-flush on recovery:** when the bus becomes reachable again (FEAT-041), parked
  items are **re-validated for current relevance** (drop self-resolved) and flushed to
  the supervisor (FEAT-042).
- Critical items additionally trigger an active push (FEAT-044).

## Acceptance Criteria
1. In standalone mode, decision/approval items are parked, not pushed.
2. `regin chat` opens with the health line + actionable items; the user can
   approve/decide from the greeting.
3. On bus recovery, parked items are re-validated and flushed; self-resolved items are
   dropped; unit-tested.
