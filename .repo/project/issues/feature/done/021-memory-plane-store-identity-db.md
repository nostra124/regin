---
id: FEAT-021
type: feature
priority: high
complexity: L
estimate_tokens: 70k-120k
estimate_time: 120-180min
phase: done
status: done
milestone: 0.6.0
spawned_from: DISC-017
depends_on: FEAT-006
---

# FEAT-021 — Memory plane store: portable `identity.db` + full schema

## Description
**As** regin
**I want** a dedicated, portable SQLite store (`identity.db`) for the whole memory
plane
**So that** my self-improving memory can be lifted from one container and dropped
into another while the machine-local ITIL/audit/KPI state stays behind.

This is the foundation of the memory plane (DISC-017): a single copyable file is the
physical portability seam, separate from the machine-local `regin.db`.

## Design

### Approach
New `regin-core/src/identity_db.rs` module following the same pattern as `db.rs`:
`init_identity_db(path)` → `init_identity_schema(conn)`. The schema covers all
DISC-017 tables: `identity_meta` (key/value metadata seeded on first bootstrap),
episodic tier (`episodes`, `sessions`, `transcripts`), long-term tier (`topics`,
`memories`), FTS5 virtual tables (`memories_fts`, `transcripts_fts`) with sync
triggers, and indexes. Schema version tracked in `identity_meta`.

### Files touched
- `regin-core/src/identity_db.rs` — new module (store + schema + bootstrap)
- `regin-core/src/config.rs` — add `identity_db_path()` (alongside `db_path()`)
- `regin-core/src/lib.rs` — add `pub mod identity_db;`
- `Cargo.toml` — add `"fts5"` to `rusqlite` features

### Dependencies
- `rusqlite` with `fts5` feature (for `CREATE VIRTUAL TABLE ... USING fts5`)
- `uuid` (already a workspace dependency) for seeding `identity_id`

### Open questions
None; the schema per DISC-017 is fully specified and the AC are concrete.

## Implementation
- New `identity.db` opened alongside `regin.db`, in-stack via `rusqlite`; path under
  the XDG data dir (FEAT-008 conventions), distinct from the machine DB.
- Create the full DISC-017 schema with `CREATE TABLE IF NOT EXISTS` +
  `add_column_if_missing` idempotent migrations, `TEXT` uuid PKs, RFC3339 timestamps:
  - **Episodic tier:** `episodes` (with `kind`, `host`, `importance`, `state` =
    `new|consolidated`), `sessions`, `transcripts`.
  - **Long-term tier:** `topics` (slug, hierarchy via `parent_id`, `summary`,
    `host`, `pinned`), `memories` (evolved from FEAT-006: `topic_id`, `category`,
    `tier` = `medium|long`, `host`, `repo_key`, `source`, `strength`, `trust_score`,
    `retrieval_count`, `helpful_count`, `pinned`, `last_seen`, `last_retrieved`,
    `embedding`).
  - **Search:** `memories_fts` + `transcripts_fts` (FTS5, external-content) with
    `ai/ad/au` sync triggers.
  - **Indexes** per the DISC (`idx_episodes_state/host`, `idx_memories_topic/tier/
    host/lastret`).
  - **`identity_meta`** key/value (identity_id uuid, name, schema_version,
    created_at).
- A versioned schema bootstrap creates the DB on first run and stamps
  `identity_meta.schema_version`.
- This ticket delivers the **store + schema + open/bootstrap** only; capture,
  consolidation, retrieval, migration, and portability are FEAT-022..027.

## Acceptance Criteria
1. On first run `identity.db` is created with every table, FTS index, trigger, and
   index from the DISC-017 schema; re-running is idempotent.
2. `identity_meta` holds an `identity_id` (uuid), `name`, and `schema_version`.
3. The store is physically separate from `regin.db` (distinct file/handle).
4. FTS triggers keep `memories_fts` / `transcripts_fts` in sync on insert/update/
   delete (unit-tested).
5. Schema migrations are additive and idempotent; unit-tested against a fresh and a
   re-opened DB.

## Resolution

Implemented in `regin-core/src/identity_db.rs` — new module with `init_identity_db(path)`,
`init_identity_schema(conn)`, `meta_get()`, and full DISC-017 schema (identity_meta,
episodic tier: episodes/sessions/transcripts, long-term tier: topics/memories, FTS5
virtual tables with sync triggers, 9 indexes). Added `identity_db_path()` to config.rs
and exposed the module in lib.rs. No rusqlite feature change needed — FTS5 ships in
the bundled SQLite build by default.

All 11 unit tests pass (187 total in workspace), clippy-clean for new code.

Acceptance check:
1. ✅ `init_identity_db` creates all tables, FTS indexes, triggers, and indexes; re-running is idempotent.
2. ✅ `identity_meta` holds `identity_id` (uuid), `name`, and `schema_version`.
3. ✅ Store is physically separate from `regin.db` (distinct file at `identity_db_path()`).
4. ✅ FTS triggers keep `memories_fts` / `transcripts_fts` in sync on insert/update/delete.
5. ✅ Schema is additive and idempotent; tested against both in-memory and file-backed DB.
