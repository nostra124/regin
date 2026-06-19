---
id: FEAT-032
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-018
depends_on: FEAT-028
---

# FEAT-032 — Deliberation capture

## Description
**As** regin
**I want** every deliberation recorded — the plan, the Soul's vote, and the eventual
outcome
**So that** the Soul can calibrate against real results over time and reflection can
derive principles from what actually worked (FEAT-031).

This implements DISC-018 Q4 (capture & learn).

## Implementation
- **Schema (additive migration on the FEAT-021 / FEAT-023 episode store):** a
  `deliberation` episode kind whose detail records:
  - the **plan** — `intent_summary` + `steps`,
  - the **Soul vote** — `confidence` + `verdict` + `gut_reaction` (FEAT-029),
  - the **disposition** — `executed | denied | escalated`,
  - the **outcome** — back-filled after execution: `success | failure | rolled_back`,
    linked to any resulting ITIL change/incident record where applicable.
- **Write hook:** the deliberate loop (FEAT-028) writes one `deliberation` episode at
  decision time; an outcome observer back-fills `outcome` once known (on executor
  completion / subsequent incident linkage).
- **Feeds:** the consolidation loop (FEAT-024) reads `deliberation` episodes for
  principle derivation (FEAT-031); retrieval (FEAT-025) can surface similar past
  deliberations to the Mind/Soul.
- **Fail-safe:** capture is bounded and never blocks the decision path — errors are
  logged and the loop continues.

## Acceptance Criteria
1. A completed deliberation writes exactly one `deliberation` episode with plan +
   vote + disposition.
2. The `outcome` field is back-filled after execution (`success | failure |
   rolled_back`) and links the relevant ITIL record where applicable.
3. Capture failures are logged and do not block or crash the decision loop.
4. The consolidation loop can query `deliberation` episodes.
5. Unit-tested (write at decision time + later outcome back-fill).
