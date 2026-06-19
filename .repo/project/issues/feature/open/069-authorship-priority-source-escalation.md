---
id: FEAT-069
type: feature
priority: high
complexity: M
estimate_tokens: 60k-100k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-061
---

# FEAT-069 — Authorship, prioritization & source-routed escalation

## Description
**As** an operator (and dvalin, and regin itself)
**I want** to set/adjust objectives & goals and have trouble routed back to me
**So that** intent has clear ownership and infeasibility reaches the right decider.

## Implementation
- **Authorship**: create/change objectives & goals from any **source** — a human, a
  **dvalin-hierarchy LLM** (over the bus), or **regin itself**; every create/change is
  **approval-gated** (like to-be-state edits).
- **Prioritization**: each intent's priority drives conflict arbitration (FEAT-062)
  and selection under the cost budget (objectives = hard constraints, goals =
  prioritized targets).
- **Source-routed escalation**: on 🔴 (FEAT-066), escalate to the intent's **source**
  via the DISC-010 channels — dvalin supervisor over the bus, or human via
  login-greeting / critical push — presenting the three remedies (provide resources /
  adjust / replan).
- **Surfacing**: RAG + goal/objective progress in `regin metrics` and the login
  greeting; CLI verbs (`regin goal …`, `regin objective …`).

## Acceptance Criteria
1. Intents can be authored by human / dvalin / regin, each approval-gated, with
   priority + source persisted.
2. A red goal escalates to its source over the correct channel with the three
   remedies (unit-tested with a fake bus + fake greeting).
3. RAG/progress appears in metrics + greeting; CLI verbs manage intents.
