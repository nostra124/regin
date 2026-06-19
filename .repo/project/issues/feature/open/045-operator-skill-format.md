---
id: FEAT-045
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-012
depends_on: FEAT-007
---

# FEAT-045 — Operator-skill format + authoring

## Description
**As** regin (and skill authors)
**I want** a structured operator-skill format that closes the loop
**So that** a skill can monitor a domain, judge it against the to-be state, and offer
remediations.

## Implementation
- An operator skill ↔ one to-be-state domain, bundling:
  - a **monitor** (gather signals + LLM-judge vs the domain's to-be state, FEAT-034),
  - the **default to-be-state domain file** (user-editable, FEAT-033),
  - a **remediation playbook** of candidate fixes, each tagged for a DISC-009 lane
    (FEAT-037).
- The skills engine runs the monitor, raises incidents on deviation (FEAT-035), and
  offers remediations to the guardrail.
- **User-overridable** via the existing user-over-system layering; the **FEAT-007
  skill-creation flow** extended to scaffold an operator skill (monitor + to-be-state
  + remediations).

## Acceptance Criteria
1. An operator skill loads with its monitor, to-be-state file, and remediation
   playbook; the engine runs the monitor and raises an incident on deviation.
2. A remediation is offered to the three-lane engine with its lane tag.
3. A user skill overrides a system skill by name; `regin` can scaffold a new operator
   skill; unit-tested.
