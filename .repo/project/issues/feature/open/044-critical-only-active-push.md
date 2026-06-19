---
id: FEAT-044
type: feature
priority: medium
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-010
depends_on: FEAT-043
---

# FEAT-044 — Critical-only active push

## Description
**As** an operator
**I want** truly critical standalone items to reach me actively
**So that** an emergency doesn't wait unseen until my next login.

## Implementation
- An **opt-in, off-by-default**, severity-gated push channel for **critical** items
  only; non-critical items still wait for the login greeting (FEAT-043).
- Channel(s): ntfy / webhook / email, configured via `regin config`.
- Severity gate so the channel never becomes noisy; deduplicate/rate-limit repeats.
- Push failures fall back to the parked/greeting path (never lose the item).

## Acceptance Criteria
1. With the channel disabled (default), nothing is pushed; items only appear at login.
2. With it enabled, a critical item is pushed; a non-critical item is not.
3. A push failure still leaves the item parked for the greeting; repeats are
   rate-limited; unit-tested with a fake channel.
