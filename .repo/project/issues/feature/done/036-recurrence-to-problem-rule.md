---
id: FEAT-036
type: feature
priority: medium
complexity: S
estimate_tokens: 30k-60k
estimate_time: 45-90min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-011
depends_on: FEAT-035
---

# FEAT-036 — Recurrence-to-problem rule

## Description
**As** regin
**I want** a recurring incident to escalate into a problem past a threshold
**So that** chronic deviations get root-caused instead of repeatedly patched.

## Implementation
- Track incident recurrence per domain/signature.
- Threshold = a global config default `operator.recurrence_threshold` (default **3**),
  **overridable per-domain** in the to-be-state doc (FEAT-033).
- When recurrence for a signature exceeds its effective threshold, open a **problem**
  and link the incidents via `problem_incidents` (FEAT-035).
- The problem carries the recurrence evidence; its real fix is a change that rides out
  of the problem (`change.problem_id`).

## Acceptance Criteria
1. An incident recurring beyond the effective threshold opens a problem linking the
   recurrences.
2. A per-domain override in the to-be-state doc takes precedence over the global
   default.
3. Below threshold, no problem is opened; unit-tested with a fake clock/sequence.
