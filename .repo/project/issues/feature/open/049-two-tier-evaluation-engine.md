---
id: FEAT-049
type: feature
priority: high
complexity: L
estimate_tokens: 70k-120k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-015
depends_on: FEAT-034
---

# FEAT-049 — Two-tier evaluation engine

## Description
**As** regin
**I want** a periodic LLM review tier plus a cheap deterministic check tier
**So that** monitoring is thorough where judgement is needed and cheap where it isn't —
"senseful full automation".

## Implementation
- **LLM review tier:** judges deviation-worth against the three-layer to-be state
  (FEAT-033/034) on a periodic cadence.
- **Deterministic check tier:** cheap, LLM-free checks (the promotion target, FEAT-051)
  run frequently; both tiers feed the incident/problem flow (FEAT-035).
- The deterministic tier is the **degraded-mode fallback** during LLM outages
  (FEAT-048).
- Add the **"senseful full automation" directive** to regin's operator-plane system
  prompt (the baseline operator directive, cf. `regin-core/src/context.rs`).

## Acceptance Criteria
1. Both tiers run and feed incidents/problems; the LLM tier judges worth-against-intent,
   the deterministic tier evaluates fast rules.
2. The operator system prompt carries the "senseful full automation" directive.
3. The engine is bounded/fail-safe; unit-tested with a fake LLM + fake checks.
