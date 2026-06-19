---
id: FEAT-029
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-018
depends_on: FEAT-028
---

# FEAT-029 — The Soul gate (values-grounded vote + veto)

## Description
**As** regin
**I want** a deliberately **starved**, values-grounded **Soul** that votes on the
Mind's plan (approve / revise / veto)
**So that** consequential plans are checked against who I truly am before they
execute — a gut-check the Mind cannot out-argue.

## Implementation
- The **Soul** is a tool-less LLM call with a constrained system prompt: *"You are
  the conscience. Given the plan's intent and these values, return a gut reaction."*
  - **Input:** `Plan.intent_summary` (from FEAT-028) + the **active values
    grounding** — the identity-core values **unioned with the active Persona's value
    overlay** (FEAT-030), i.e. pinned + human-authored + `principle` category + topic
    summaries.
  - **Deliberately withheld:** the Mind's full reasoning chain, the tool list, the
    environment. That starvation is what makes the vote a *feeling*, not a second
    round of logic.
- **Output (structured):** `confidence` (0–1), one-line `gut_reaction`, `verdict`
  ∈ `{approve, revise, veto}`.
- **Gate logic:**
  - `approve` **and** `confidence ≥ decision.deliberate.confidence_threshold`
    (default 0.7) → gate passes.
  - `approve` below threshold, or `revise` → return `gut_reaction` to the Mind to
    re-plan (FEAT-028 loop).
  - `veto` → hard stop, no execution.
- **Deadlock:** after `max_rounds` without a passing approve → **default-deny +
  escalate** to a human via the escalation bridge (FEAT-015), routed by runtime mode
  (DISC-010).
- **Audit:** every vote (`plan_id`, `confidence`, `verdict`, `gut_reaction`) is
  emitted for deliberation capture (FEAT-032).

## Acceptance Criteria
1. The Soul prompt contains only intent + values: a spy asserts the Mind's
   reasoning, tool list, and environment are absent.
2. `approve` with `confidence ≥ threshold` passes the gate; below threshold is
   treated as `revise`.
3. `veto` fails the gate — no execution — and raises an escalation.
4. `max_rounds` without approval → default-deny + escalation.
5. Each vote is persisted for capture; unit-tested with a fake LLM returning
   scripted verdicts.
