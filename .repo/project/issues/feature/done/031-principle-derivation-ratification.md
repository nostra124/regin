---
id: FEAT-031
type: feature
priority: high
complexity: L
estimate_tokens: 60k-110k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-018
depends_on: FEAT-030
---

# FEAT-031 — Principle derivation & ratification

## Description
**As** regin
**I want** reflection to **propose** new principle candidates from how my past
deliberations actually turned out, and a **human to ratify** them before they become
values
**So that** my conscience grows from validated experience without the Mind authoring
the very values it is checked against.

This is the **propose** + **promote** stages of DISC-018's value pipeline (Q5); the
**seed** stage is FEAT-030.

## Implementation
- **Schema (additive migration on the FEAT-021 `identity.db`):** `principle` category
  with `principle_status` ∈ `{candidate, active, retired}` and `source` ∈
  `{human, reflection}`.
- **Propose (reflection):** extend the consolidation loop (FEAT-024) to read
  `deliberation` episodes (FEAT-032) — plan + Soul vote + **outcome** — and surface
  principle **candidates** where an outcome pattern **recurs** (e.g. overriding a
  low-confidence Soul vote on irreversible changes repeatedly led to bad outcomes →
  candidate: "don't auto-apply irreversible changes without a backout"). Candidates
  are written `status=candidate, source=reflection` and are **never auto-activated**
  and **never read by the Soul**.
- **Promote (human-ratified):**
  - `regin soul principles list --candidates` — review proposals (with the evidence
    that produced them).
  - `regin soul principles ratify <id>` → `active`; `reject <id>` → `retired`.
  - Ratification may also be routed as an escalation (FEAT-015 / DISC-010) so a
    human/supervisor is actively prompted.
- **Stickiness:** active principles are pinned-like with **slow decay**; retiring an
  active principle requires explicit human action or overwhelming counter-evidence;
  `source=human` seed values (FEAT-030) **never** decay.
- **Grounding:** only `active` principles + the human charter feed the Soul's
  grounding (FEAT-029).

## Acceptance Criteria
1. Reflection over `deliberation` episodes produces `candidate` principles only —
   never `active`, and never from a single instance (recurrence threshold enforced).
   Unit-tested.
2. `ratify` promotes `candidate` → `active`; `reject` → `retired`.
3. A `candidate` is never surfaced to the Soul until ratified.
4. Active principles decay slowly; `source=human` seed values never decay; retiring
   is gated.
5. Unit-tested with a fake clock and scripted deliberations.
