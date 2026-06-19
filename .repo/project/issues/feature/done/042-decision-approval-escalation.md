---
id: FEAT-042
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-010
depends_on: FEAT-041
---

# FEAT-042 — Decision/approval escalation (org, over the bus)

## Description
**As** regin in an org
**I want** to route changes needing approval and problems needing a decision to the
supervisor over the bus
**So that** a human/supervisor can unblock me without me overstepping.

## Implementation
- A **second escalation flavour** atop the FEAT-015 bridge: where FEAT-015 asks the
  *dev plane* to mint a BUG/FEAT ticket, this asks a *human/supervisor* for a
  **decision/approval**.
- When effective mode is **org** (FEAT-041): send `pending_approval` changes (FEAT-037)
  and decision-needing problems to the supervisor over the bus (FEAT-010); carry the
  context needed to decide (the proposed change, its backout, the deviation).
- Receive the approve/reject verdict and resume the change lifecycle (apply on approve;
  record on reject).
- This realizes DISC-009's "approval routing" for the org case (standalone case =
  FEAT-043).

## Acceptance Criteria
1. In org mode, a `pending_approval` change is sent to the supervisor with sufficient
   decision context.
2. An approve verdict applies the change; a reject records it without applying.
3. The escalation is tagged as a decision/approval request (distinct from FEAT-015
   ticket-mint); unit-tested with a fake bus.
