---
id: FEAT-005
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 45-90min
phase: done
status: done
spawned_from: DISC-002
---

# Episodic memory tier (capture of runs / incidents / chats)

## Description
**As** regin
**I want** a short-term episodic memory tier recording what happened
**So that** the reflective loop (FEAT-006) has raw material to distil into durable
semantic memories.

The existing `memories` table becomes the **semantic** tier; this adds the
**episodic** tier beneath it.

## Implementation
- New `episodes` table: id, kind (`task_run|incident|chat|change`), ref_id
  (the related record's id), summary, detail (json/text), created_at,
  reflected (bool, default false).
- `db` functions: `episode_record`, `episode_recent(unreflected, limit)`,
  `episode_mark_reflected(ids)`, plus a roll-off `episode_prune(before)`.
- Capture points (write episodes):
  - task run finishes (status + output preview) — in `regind` scheduler + exec.
  - incident opened/updated (FEAT-004).
  - optionally, notable chat outcomes.
- Retention: `memory.episodic_retention_days` setting; the scheduler prunes
  episodes older than the window (after they've been reflected).

## Acceptance Criteria
1. Each completed scheduled run writes one episode with status + summary.
2. `episode_recent` returns only unreflected episodes, newest first, bounded by
   limit.
3. Pruning removes episodes older than the configured retention and never
   removes unreflected ones.
4. Round-trip + prune unit tests with a fake clock.
5. No semantic memories are written here — that is FEAT-006.
