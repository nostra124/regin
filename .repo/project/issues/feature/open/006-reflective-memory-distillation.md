---
id: FEAT-006
type: feature
priority: high
complexity: L
estimate_tokens: 60k-110k
estimate_time: 90-150min
phase: open
status: open
depends_on: FEAT-005
spawned_from: DISC-002
---

# Reflective distillation: episodic → semantic, with reinforcement & decay

## Description
**As** regin
**I want** regind to periodically reflect over recent episodes and distil durable
semantic memories, reinforcing recurring signals and letting stale ones decay
**So that** regin improves over time without a human writing every memory.

This is the "Hermes" self-improving loop.

## Implementation
- Add reinforcement bookkeeping to the semantic `memories` table:
  `strength` (int, default 1), `last_seen`, `source` (`human|reflection`).
  Idempotent migration (additive columns).
- Reflection job in `regind` (scheduled; cadence from `memory.reflect_interval`):
  1. Pull a bounded window of unreflected episodes (`episode_recent`).
  2. Prompt the LLM to propose new/updated semantic memories (category +
     content), explicitly asked to merge with existing similar memories.
  3. For each proposal: if it matches an existing memory (fuzzy/category match),
     reinforce (`strength += 1`, bump `last_seen`); else insert with
     `source=reflection`.
  4. Mark the episodes reflected.
- Decay: on reflection, decrement `strength` of reflection-sourced memories not
  seen within a decay window; drop at strength 0 (never auto-delete
  `source=human`).
- Context injection: order injected memories by strength so the strongest
  surface first; keep within a context budget.
- All auto-memories remain auditable/reversible via existing `memory` verbs;
  `memory list` shows source + strength.

## Acceptance Criteria
1. Running reflection over episodes produces semantic memories and marks those
   episodes reflected (no double-counting on the next run).
2. A signal seen across multiple reflection cycles increases `strength`; a
   reflection memory unseen past the decay window loses strength and is dropped
   at 0.
3. `source=human` memories are never decayed or auto-deleted.
4. Reflection is bounded in tokens (window cap) and fails safe (errors logged,
   scheduler continues).
5. `memory list` surfaces `source` and `strength`; unit tests cover
   reinforcement and decay with a fake clock.
