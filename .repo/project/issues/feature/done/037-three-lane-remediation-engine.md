---
id: FEAT-037
type: feature
priority: high
complexity: L
estimate_tokens: 80k-130k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-009
depends_on: FEAT-035
---

# FEAT-037 — Three-lane remediation engine

## Description
**As** regin
**I want** to route each candidate fix into one of three lanes
**So that** the operator loop *closes* — acting on safe fixes, asking before risky
ones, escalating what's beyond it — instead of only reporting.

## Implementation
- For a deviation → incident (FEAT-034/035), regin proposes a candidate fix and the
  LLM judges its risk (hybrid, DISC-009): a declarative **safe-action fast-path** for
  pre-blessed reversible ops + LLM judgement for novel fixes, all bounded by the
  capability ceiling (FEAT-038).
- Route into a lane:
  - **safe + reversible** → **auto-apply** the change (subject to the safe-lane gate,
    FEAT-039).
  - **uncertain / destructive / wide blast radius** → change `pending_approval`; get
    approval before apply (routed by FEAT-042/043).
  - **out of regin's control** → don't attempt → open a **problem** + escalate.
- Every applied change records what it did + its backout (FEAT-039) as an ITIL change.
- Worked example: `/` at 95% → "delete temp files" (safe) → auto-apply; "edit logging
  config" (uncertain) → pending_approval.

## Acceptance Criteria
1. A safe/reversible candidate auto-applies; an uncertain one becomes
   `pending_approval`; an out-of-control one opens a problem + escalation.
2. Lane routing respects the capability ceiling (FEAT-038) and the safe-lane gate
   (FEAT-039).
3. Each auto-applied fix is recorded as a change with its backout plan.
4. Unit-tested across all three lanes with a fake LLM judge.
