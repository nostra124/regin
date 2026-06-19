---
id: FEAT-047
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-013
depends_on: FEAT-003
---

# FEAT-047 — Per-skill scheduling + jitter

## Description
**As** regin
**I want** each operator skill to run on its own cadence, spread out over time
**So that** monitoring matches each domain's rhythm without bunching LLM cost/load.

## Implementation
- Each operator skill (FEAT-045) declares a **default cadence** (existing cadence
  strings); **user/config override**; optional per-domain tune in the to-be-state doc
  (FEAT-033).
- **Automatic jitter/staggering** of scheduled runs so skills (and their LLM calls)
  don't all fire together — smooths cost and load.
- Promoted deterministic checks (FEAT-051) schedule on the same engine at their own
  (cheaper, more frequent) cadence.

## Acceptance Criteria
1. A skill runs on its declared default cadence; a user/config override and a
   to-be-state per-domain tune both take precedence in that order.
2. Concurrent due-times are staggered by jitter (no thundering herd); unit-tested.
3. Deterministic checks and LLM skills coexist on the scheduler.
