---
id: FEAT-041
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-010
depends_on: FEAT-010
---

# FEAT-041 — Effective-mode detection (org vs standalone)

## Description
**As** regin
**I want** to know whether I'm effectively in an org (bus reachable) or standalone
**So that** escalations route correctly even when a configured bus is down.

## Implementation
- **Effective mode = configured target (bus/persona) AND recent reachability**
  (Variant C): a configured-but-down bus falls back to standalone; transient blips
  don't flip the mode (debounced on last-send health).
- Add a runtime reachability probe + last-send health state to the bus client
  (FEAT-010); today mode is inferred only from persona config.
- Expose `effective_mode()` to the loop (escalation, greeting, approval routing).

## Acceptance Criteria
1. Bus configured + recently reachable ⇒ org; configured + unreachable ⇒ standalone;
   not configured ⇒ standalone.
2. A single transient failure does not flip mode (debounce); a sustained outage does.
3. `effective_mode()` is consumed by escalation/greeting; unit-tested with a fake bus
   health source.
