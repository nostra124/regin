---
id: FEAT-024
type: feature
priority: high
complexity: L
estimate_tokens: 80k-130k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-017
depends_on: FEAT-023
---

# FEAT-024 — Consolidation pipeline (Curator)

## Description
**As** regin
**I want** a consolidation pass that turns raw episodes and transcripts into durable,
topic-organized knowledge — resolving conflicts rather than accumulating duplicates
**So that** the identity gets *wiser*, not just *bigger*, over time.

This replaces the flat FEAT-005/006 episodic→semantic loop with the DISC-017 tiered
Curator (modal model + ACT-R).

## Implementation
- **Scheduled pass in `regind`** (cadence from `memory.reflect_interval`) reading
  `episodes.state='new'` and un-consolidated transcripts in a bounded window.
- **Extract:** the LLM proposes facts into `memories` (`tier='medium'`,
  `source=reflection`) with `category`, `tags`, and a `topic_id`; updates/creates
  `topics.summary` (index + sub-document style).
- **Interference resolution:** each proposal is reconciled against existing similar
  facts via **ADD / UPDATE / DELETE / NOOP** — supersede conflicting facts instead of
  storing parallel copies. Reinforcement bumps `strength` / `last_seen`.
- **Promotion:** medium facts reinforced past a threshold promote to `tier='long'`.
- **Forgetting:** `memory_decay` (extended from FEAT-006) decays reflection facts —
  medium faster than long — dropping at strength 0; `episode_prune` drops
  consolidated episodes past retention; `source=human` / `pinned` rows and
  transcripts are protected (transcripts kept cold; summarized into
  `sessions.summary`).
- **State:** processed episodes → `state='consolidated'` (`consolidated_at` set) so
  the next pass never double-counts.
- `host` is carried through (a host-scoped episode yields host-scoped facts).
- Bounded in tokens; fails safe (errors logged, scheduler continues).

## Acceptance Criteria
1. A consolidation pass over `new` episodes/transcripts produces `medium` facts +
   topic summaries and marks the episodes `consolidated` (no double-count next run).
2. A conflicting fact is **superseded** (UPDATE/DELETE), not duplicated; NOOP leaves
   memory unchanged; unit-tested across all four interference actions.
3. A fact reinforced past the threshold promotes `medium`→`long`.
4. Decay drops stale reflection facts at strength 0; `human`/`pinned`/transcripts are
   never decayed or pruned.
5. The pass is token-bounded and fails safe; unit-tested with a fake LLM + fake clock.
