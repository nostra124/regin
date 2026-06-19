---
id: FEAT-034
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-008
depends_on: FEAT-033
---

# FEAT-034 — Observed-vs-target monitoring evaluation

## Description
**As** regin
**I want** monitoring redefined as *observed vs target* against the to-be state
**So that** an incident means a real deviation from intent — not merely that a job
errored.

Supersedes the "run-errored ⇒ incident" framing in `monitoring-triage.md` / FEAT-004.

## Implementation
- Evaluation compares observed signals (from operator skills, DISC-012) against the
  three-layer target (markdown intent + structured assertions + implicit monitor
  thresholds, FEAT-033).
- **Deviation is LLM-judged**, not raw events: the LLM judges whether an observation
  is worth-against-intent before it becomes an incident (cheap deterministic checks
  feed in too — see FEAT-049).
- A judged deviation raises an incident (DISC-011 lifecycle); agreement with target is
  a no-op.
- Replaces the FEAT-004 trigger path; run errors become one *input* to judgement, not
  an automatic incident.

## Acceptance Criteria
1. An observation that deviates from the target raises an incident; one within target
   does not.
2. A job error that does not breach intent does **not** auto-raise an incident
   (regression vs FEAT-004 framing).
3. Evaluation reads all three target layers; unit-tested with a fake LLM judge.
4. Deviation judgements are bounded/fail-safe (logged; loop continues).
