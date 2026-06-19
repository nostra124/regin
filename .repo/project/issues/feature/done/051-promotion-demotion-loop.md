---
id: FEAT-051
type: feature
priority: high
complexity: L
estimate_tokens: 70k-120k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-015
depends_on: FEAT-049
---

# FEAT-051 — Promotion + demotion loop (derived-checks store)

## Description
**As** regin
**I want** to distil stable LLM verdicts into cheap deterministic checks, and pull
them back when they're wrong
**So that** monitoring gets cheaper over time without losing accuracy.

## Implementation
- **Promotion:** autonomous, audited promotion of crystal-clear, stable LLM verdicts
  into a **separate machine-managed derived-checks store** (referencing the to-be
  state; never written into the human-authored structured layer).
- **Criteria:** regin-owned and **self-adapting**, grounded in *both*
  N-consistent + confidence and a statistical error-bound; governed by the
  promotion-error KPI (FEAT-050).
- Derived checks run on the scheduler (FEAT-047) as the cheap deterministic tier
  (FEAT-049).
- **Demotion hook:** immediate demotion on real-world contradiction/override (a
  promoted check that misses or over-fires). The periodic wide-lens re-audit lives in
  FEAT-055 (DISC-016).

## Acceptance Criteria
1. A stable, repeated LLM verdict promotes to a derived check (with audit trail) once
   criteria are met; an unstable one does not.
2. Promoted checks live in the derived-checks store, separate from human-authored
   to-be state.
3. A promoted check that is contradicted/overridden is immediately demoted; the
   promotion-error KPI updates; unit-tested with a fake clock.
