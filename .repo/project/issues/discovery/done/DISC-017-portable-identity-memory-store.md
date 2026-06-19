---
id: DISC-017
type: discovery
priority: high
status: done
complexity: L
spawned_features: [FEAT-021, FEAT-022, FEAT-023, FEAT-024, FEAT-025, FEAT-026, FEAT-027]
---

# DISC-017 — Portable agent identity: the self-improving memory store (its own SQLite DB)

## Identity-plane context

This DISC introduces an axis the operator-plane DISCs (DISC-008..011, DISC-015/016)
do not address. Those concern the **machine apparatus** regin uses to run *a
particular box* via ITIL. This DISC concerns the **agent identity** — the part of
regin that is *itself* and must survive being moved between machines/containers.

The two are strictly separate and must not share a store:

| | **Identity** (this DISC) | **Machine apparatus** (DISC-008..016, existing) |
|---|---|---|
| Portability | copyable container → container; travels with the agent | rebuilt per machine; bound to the box |
| Store | its **own SQLite DB** | the machine-local `regin.db` + files (`desired/`, `filters/`, derived checks) |
| Contents | archived sessions, documented transcripts, topic-based knowledge summaries, distilled semantic memory | ITIL records (incident/change/problem/KPI), audit results, local tools/skills/docs |
| Nature | self-improving; *what the agent has learned* | rule-driven operations of *this* machine |

Two clarifications that motivate this DISC:

- **The audit is apparatus, not identity.** The traceability/self-audit machinery
  (`operations/audit.md`, DISC-016) executes rules and raises ITIL incidents bound
  to the machine's process. It has *nothing* to do with the identity and must not
  be conflated with self-improvement.
- **"Hermes" is an external reference project**
  ([hermes-agent.nousresearch.com](https://hermes-agent.nousresearch.com),
  [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent)) — the
  *inspiration* for this memory design, not a regin subsystem. (DISC-002 / FEAT-006
  used "Hermes" as an internal nickname; that naming is loose and is not rewritten
  here per the audit's "don't retroactively rewrite shipped tickets" rule.)

## Describe

regin's self-improving memory is the substance of its identity: the more it
operates, the more it should know — and that knowledge should follow the agent,
not the machine. Today it does neither cleanly:

1. **Storage is co-located.** DISC-002 → FEAT-005 (episodic tier) + FEAT-006
   (reflective episodic → semantic distillation) put memory in the **same
   `regin.db`** as settings, task runs, ITIL records, and KPIs. Copying the
   identity to a new container would drag the old machine's incidents/changes with
   it, or require surgical extraction. There is no clean seam.

2. **The identity is thinner than it should be.** FEAT-005/006 capture *episodes*
   (run/incident/chat) and distil *semantic memories* (fact/preference/pattern/…).
   They do **not** capture what you describe as the core of the identity:
   - **Archived sessions** — the agent's own past working sessions, retained.
   - **Documented transcripts** — readable records of what was done and decided.
   - **Topic-based knowledge summaries** — durable, subject-organized knowledge
     consolidated from the above (closer to Hermes' organization than to a flat
     `memories` table).

The goal: a dedicated, **portable identity store** (its own SQLite DB) holding the
self-improving memory — sessions, transcripts, topic knowledge, and distilled
semantic memory — with a clean boundary so it can be lifted from one container and
dropped into another, while the machine-local ITIL/audit/operational state stays
behind.

## Variants considered

> First-cut framing only — to refine in guided Q&A. The decision matrix and Pro/Con
> are deliberately left for the Q&A so the options aren't prejudged.

| Variant | Summary | Key trade-off |
|---|---|---|
| A | **Second SQLite DB** (`identity.db`) alongside `regin.db`; move memory tables there, add session/transcript/topic-knowledge tables | Clean portable seam, one extra file, in-stack (rusqlite); needs a migration off `regin.db` |
| B | Keep one DB; mark identity tables and export/import a subset on copy | No second file; but export logic is fiddly and the seam is by-convention, not physical — easy to violate |
| C | Identity as files (markdown sessions/transcripts/topic docs) + a small index DB | Most human-readable / git-friendly; loses transactional consolidation and cross-linking the reflection loop wants |
| D | External store (vector DB / embeddings) for topic knowledge | Powerful recall; reintroduces the heavy dependency DISC-002 already rejected at this scale |

## Open questions — resolved (guided Q&A)

1. **Boundary.** Chats belong to the **identity**. Task runs are **dual**: they run
   operationally on the **machine** (ITIL/telemetry in `regin.db`) *and* feed the
   identity. The self-improvement loop ingests **everything the agent did and
   observed** — runs, audits, incidents, chats — as raw material and **condenses**
   it into memory. So the identity's reflection loop *reads from* the machine
   apparatus but the distilled knowledge lives in the identity store. The audit
   stays apparatus (it raises machine-bound incidents); reflection merely consumes
   its output.
2. **Store shape.** **Variant A** — one second SQLite DB, `identity.db`, holding
   everything including transcripts as `TEXT`. No files-on-disk hybrid; a single
   copyable file is the portable seam.
3. **Memory organization.** The existing flat episodic→semantic loop (FEAT-005/006)
   is **replaced** by a tiered model grounded in the modal model of memory + ACT-R
   activation, and informed by the Hermes reference (FTS5 `facts` + `retrieval_count`
   reinforcement, Curator state machine, index+sub-document topics, FTS5 session
   search with LLM summarization). See **Memory model** and **Schema** below.
4. **Portability.** Copy `identity.db` as-is **and** a `regin memory export/import`
   verb. An `identity_meta` row carries an `identity_id` (uuid) + name so two
   identities never collide on one box and one identity can span boxes. (Relation to
   the DISC-003 dwarf identity is left to that DISC; this `identity_id` is the
   memory-plane handle.)
5. **Machine-scoped knowledge.** **Tag per-host, travels with the identity.** A
   nullable `host` column on episodic + long-term rows: `NULL` = identity-global
   (applies everywhere), a set host = machine-specific knowledge that still lives in
   and travels with `identity.db` but only injects on the matching host. This is the
   concrete realization of the "runs feed identity **and** machine" boundary.
6. **Migration.** **Move and delete** — relocate `episodes` + `memories` from
   `regin.db` into `identity.db` and drop the originals, so there is a single source
   of truth immediately. One dedicated migration feature (FEAT-022).
7. **Naming.** The subsystem is the **memory plane**, paralleling the operator-plane
   DISCs. Its portable store is `identity.db`; CLI verbs live under `regin memory …`.
   "Hermes" remains the external reference project only.

## Memory model

Four tiers (modal model: sensory→short→long, plus a medium consolidation buffer):

- **Working** — the live context window. *Not persisted*; on close it becomes a
  session + transcript. No table.
- **Episodic** — raw, high-volume record of *what happened* (`episodes`) plus
  archived working sessions (`sessions`) and their readable `transcripts`.
  Rolls off after consolidation.
- **Medium-term** — freshly consolidated facts (`memories.tier = 'medium'`); decays
  faster; promoted to long-term only when reinforced.
- **Long-term** — durable, topic-indexed knowledge (`topics` index + sub-document
  summaries) and proven facts (`memories.tier = 'long'`); slow decay, pinnable.

The verbs:

- **Capture (cheap write).** Every run/audit/incident/change/chat → `episodes`
  (`state = 'new'`); a closed working session → `sessions` + `transcripts`.
- **Consolidate (Curator / reflection pass).** Reads `state = 'new'` episodes and
  un-consolidated transcripts; the LLM extracts facts into `memories`
  (`tier = 'medium'`), updates `topics.summary`, and **resolves interference** via
  ADD / UPDATE / DELETE / NOOP (supersede conflicting facts rather than accumulate
  parallel copies). Reinforced medium facts promote to `tier = 'long'`.
- **Retrieve (activation rank).** `memories_fts` / `transcripts_fts` BM25 + tag /
  topic filter, reranked by activation = f(rank, recency via `last_retrieved`,
  `retrieval_count`, `trust_score`, `strength`); each hit bumps `retrieval_count` /
  `last_retrieved` (self-reinforcing, ACT-R/HRR). A **vector path** (cosine over
  `embedding`) layers in alongside FTS — **in scope for the initial build**, which
  consciously overrides DISC-002's deferral of embeddings *for the memory plane*.
- **Forget (active decay + interference resolution).** `episode_prune` drops
  consolidated episodes past retention; `memory_decay` (extended from FEAT-006)
  decays reflection facts — medium faster than long — and drops at strength 0;
  `human`/`pinned` rows and transcripts are protected (transcripts kept cold and
  summarized into `sessions.summary`).

## Schema (`identity.db`)

Follows existing conventions: `TEXT` uuid PKs, RFC3339 `TEXT` timestamps, idempotent
`CREATE TABLE IF NOT EXISTS`, `add_column_if_missing` migrations, `source` enum +
`strength`/`last_seen` decay machinery.

```sql
-- Identity metadata (portability / collision-avoidance) — mirrors `settings`
CREATE TABLE IF NOT EXISTS identity_meta (
    key   TEXT PRIMARY KEY,        -- identity_id, name, schema_version, created_at, exported_from
    value TEXT NOT NULL
);

-- ── EPISODIC TIER ──────────────────────────────────────────────────────────
-- Migrated & extended from regin.db (FEAT-005). `state` supersedes `reflected`.
CREATE TABLE IF NOT EXISTS episodes (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,                    -- task_run|incident|change|chat|audit|session
    ref_id          TEXT,                             -- id in the source system (machine regin.db / session)
    host            TEXT,                             -- machine it happened on (NULL = host-agnostic)
    summary         TEXT NOT NULL,
    detail          TEXT,
    importance      INTEGER NOT NULL DEFAULT 1,       -- encode-time salience
    created_at      TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'new',      -- new|consolidated
    consolidated_at TEXT
);

-- Archived working sessions (the agent's own past runs/chats)
CREATE TABLE IF NOT EXISTS sessions (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL,                      -- chat|task|operator
    host          TEXT,
    title         TEXT,
    summary       TEXT,                               -- LLM session summary → cross-session recall
    started_at    TEXT NOT NULL,
    ended_at      TEXT,
    message_count INTEGER NOT NULL DEFAULT 0,
    token_count   INTEGER,
    state         TEXT NOT NULL DEFAULT 'open'        -- open|closed|consolidated
);

-- Documented transcripts (readable record per session)
CREATE TABLE IF NOT EXISTS transcripts (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    format     TEXT NOT NULL DEFAULT 'markdown',
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- ── LONG-TERM TIER ─────────────────────────────────────────────────────────
-- Topic index + sub-document summaries (Hermes index+subdoc style), hierarchical
CREATE TABLE IF NOT EXISTS topics (
    id         TEXT PRIMARY KEY,
    slug       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    parent_id  TEXT REFERENCES topics(id),
    summary    TEXT,                                  -- consolidated topic doc
    host       TEXT,                                  -- NULL = identity-global; set = machine-specific
    pinned     INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Semantic facts — evolved from regin.db `memories` (FEAT-006). Medium+long tiers.
CREATE TABLE IF NOT EXISTS memories (
    id              TEXT PRIMARY KEY,
    topic_id        TEXT REFERENCES topics(id),
    category        TEXT NOT NULL,                    -- fact|preference|pattern|project|skill|person
    content         TEXT NOT NULL,
    tags            TEXT,                             -- delimited; FTS-indexed (tag/index query path)
    tier            TEXT NOT NULL DEFAULT 'medium',   -- medium|long  (4-tier buffer)
    host            TEXT,                             -- NULL = identity-global; set = machine-scoped
    repo_key        TEXT,                             -- preserved from FEAT-008 scoping
    source          TEXT NOT NULL DEFAULT 'human',    -- human|reflection
    strength        INTEGER NOT NULL DEFAULT 1,       -- reinforcement count (decay unit)
    trust_score     REAL    NOT NULL DEFAULT 0.5,
    retrieval_count INTEGER NOT NULL DEFAULT 0,       -- ACT-R/HRR reinforcement on recall
    helpful_count   INTEGER NOT NULL DEFAULT 0,
    pinned          INTEGER NOT NULL DEFAULT 0,       -- never auto-decays
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    last_seen       TEXT,                             -- last consolidation touch
    last_retrieved  TEXT,                             -- last successful recall (recency term)
    embedding       BLOB                              -- vector recall (in scope; NULL until embedded)
);

-- ── SEARCH (FTS5 + sync triggers) — replaces LIKE-based memory_search ────────
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts    USING fts5(content, tags, content='memories', content_rowid='rowid');
CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(content, content='transcripts', content_rowid='rowid');
-- triggers memories_ai/ad/au and transcripts_ai/ad/au keep the indexes in sync

-- ── INDEXES ────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_episodes_state   ON episodes(state, created_at);
CREATE INDEX IF NOT EXISTS idx_episodes_host    ON episodes(host);
CREATE INDEX IF NOT EXISTS idx_memories_topic   ON memories(topic_id);
CREATE INDEX IF NOT EXISTS idx_memories_tier    ON memories(tier, source);
CREATE INDEX IF NOT EXISTS idx_memories_host    ON memories(host);
CREATE INDEX IF NOT EXISTS idx_memories_lastret ON memories(last_retrieved);
```

## Decision matrix

| Criterion | Weight | A (2nd DB) | B (one DB, export subset) | C (files + index) | D (external vector store) |
|---|---|---|---|---|---|
| Clean portable seam | high | ✓ | ✗ | ~ | ~ |
| Single copyable artifact | high | ✓ | ✓ | ✗ | ✗ |
| Transactional consolidation + cross-linking | high | ✓ | ✓ | ✗ | ~ |
| Fits existing rusqlite/LLM stack | med | ✓ | ✓ | ~ | ✗ |
| Minimal new dependencies | med | ✓ | ✓ | ✓ | ✗ |

## Decision

**Chosen:** **Variant A — a dedicated, portable `identity.db`** holding the whole
memory plane, with a **4-tier model** (working / episodic / medium / long-term),
**topic-indexed + tagged** long-term knowledge, **activation-ranked FTS + vector**
retrieval, and **active decay + interference resolution**.

**Why:** A second SQLite file is the only physical (not by-convention) seam that
lets the identity be lifted between containers while the machine-local
ITIL/audit/KPI state stays behind; it keeps the transactional consolidation and
cross-linking the reflection loop needs, and rides the existing rusqlite/LLM stack.
The tiered model replaces the flat FEAT-005/006 loop with one grounded in the modal
model of memory and ACT-R activation (and the Hermes reference): deep-encoded facts
organized by topic, reinforced on recall, and actively forgotten to keep
signal-to-noise high. Per-host tagging keeps a box's quirks from bleeding across
machines while still travelling with the agent. Vector recall is pulled into the
initial build (a deliberate, scoped override of DISC-002's embedding deferral, *for
the memory plane only*) because topic-based recall benefits materially from semantic
search; it remains additive to FTS, so the store still works if embeddings are
absent.

## Spawned features

- **FEAT-021** — Memory plane store: portable `identity.db` + full schema (tiers,
  `topics`, `sessions`/`transcripts`, FTS5 + triggers, per-host `host` scoping,
  `identity_meta`).
- **FEAT-022** — Migrate `episodes` + `memories` out of `regin.db` into
  `identity.db` (move and delete — single source of truth).
- **FEAT-023** — Session archival + transcript capture (working session →
  `sessions` + `transcripts`, with LLM session summary).
- **FEAT-024** — Consolidation pipeline (Curator): episodes/transcripts → medium
  facts + topic summaries; interference resolution (ADD/UPDATE/DELETE/NOOP);
  medium→long promotion; decay/forgetting.
- **FEAT-025** — Activation-ranked retrieval: FTS5 + tag/topic filter + ACT-R/HRR
  rerank (recency + `retrieval_count` + `trust_score` + `strength`) with recall
  reinforcement.
- **FEAT-026** — Vector/embedding recall: embedding pipeline + cosine over
  `embedding`, layered additively onto FTS (in the initial milestone).
- **FEAT-027** — Portability verbs: `regin memory export/import` + `identity_meta`
  identity handle / collision handling.
