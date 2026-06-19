---
id: FEAT-022
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-017
depends_on: FEAT-021
---

# FEAT-022 — Migrate `episodes` + `memories` out of `regin.db` into `identity.db`

## Description
**As** regin
**I want** the existing `episodes` and `memories` relocated from `regin.db` into
`identity.db` and the originals dropped
**So that** there is a single source of truth for the memory plane immediately, with
no by-convention seam to violate.

DISC-017 decision: **move and delete** (not copy-and-leave).

## Implementation
- One-shot, idempotent migration run at startup when legacy memory tables are detected
  in `regin.db`:
  1. Read all `episodes` + `memories` rows from `regin.db`.
  2. Map them onto the FEAT-021 schema (e.g. FEAT-006's `reflected` flag →
     `episodes.state` `new|consolidated`; preserve `source`, `strength`, `last_seen`,
     `repo_key`, `pinned`; set `tier='long'` for established `source=human` /
     reinforced rows, `medium` otherwise; `host=NULL` = identity-global).
  3. Insert into `identity.db` within a transaction; populate FTS via triggers.
  4. On verified success, **drop** `episodes` + `memories` (and the old
     `memories_fts` if present) from `regin.db`.
- Guard with a completion marker in `identity_meta` so the migration runs once and is
  safe to re-invoke.
- Fail safe: if the copy/verify step fails, do **not** drop the originals; log and
  abort so no data is lost.

## Acceptance Criteria
1. After migration, every legacy `episodes` / `memories` row is present in
   `identity.db` with fields correctly mapped (FEAT-006 `reflected` → `state`).
2. The originals are removed from `regin.db` only after a successful, verified copy.
3. The migration is idempotent — re-running is a no-op (completion marker honoured).
4. A simulated failure mid-migration leaves `regin.db` intact (no data loss).
5. Unit-tested with a seeded legacy `regin.db` fixture.
