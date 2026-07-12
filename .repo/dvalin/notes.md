# regin — agent notes

This file is the agent's scratch space: decisions, quirks, constraints, and
open questions discovered while working. It is **never overwritten by the
toolchain**. Read it at the start of every session; append to it at the end.

## Standing rules (settled — do not relitigate)
- **Roadmap = the collective milestone files. There is intentionally NO
  `ROADMAP.md`.** The roadmap is the set of `MILESTONE-*.md` files under
  `.repo/project/issues/`, read together. Do **not** propose, create, or ask
  about a `ROADMAP.md`. (Same convention as dvalin.)
- **`.repo/dvalin/` is the workflow-engine's per-repo area** (notes, decisions,
  engine instructions) — named after the engine (dvalin), NOT after the repo.
  Same in every repo dvalin manages. Never use `.repo/<reponame>/`.
- **regin's own per-repo additions live OUTSIDE the repo.** Anything regin adds
  for a repo — additional memories, special skills, context/instructions — is
  stored in regin's XDG store (SQLite), **keyed by the repo's filesystem path**,
  never committed into the repo. The repo only carries `.repo/dvalin/` +
  `.repo/project/`. (See FEAT-008.)

## How to use this file
- Append new findings under a dated heading; never silently rewrite history.
- Record *why* a non-obvious decision was made, not just *what*.
- Park deferred work as a ticket in `.repo/project/issues/`, then note it here.

## Project quick facts
- Rust workspace, edition 2024: `regin-core` (lib), `regind` (daemon), `regin-cli` (`regin` binary).
- Thin-client CLI ↔ `regind` daemon over a Unix socket; daemon auto-starts on first use.
- All state in SQLite (`<XDG_DATA_DIR>/regin/regin.db`). **No config files** — `regin config set ...`.
- LLM access is via the NanoGPT API; the CLI holds no LLM logic.
- Tools available to the agent loop: `bash`, `read_file`, `write_file`, `edit_file`, `web_search`.
- Skills resolve user-over-system: `~/.config/regin/skills/` overrides `/usr/share/regin/skills/`.
- Chat loads per-repo context + memories + skills from regin's XDG store, keyed
  by repo path (FEAT-008). Legacy in-repo `.repo/regin/context.md` is being retired.

## Methodology
- This repo follows the V-Model methodology in `.repo/project/skills/`. Start at
  [`AGENTS.md`](../../AGENTS.md) and the profile at `.repo/project/profile.md`.
- Tickets live under `.repo/project/issues/` (FEAT / BUG / DISC / AUDT). One
  ticket → one PR → one merge commit.

## Open questions / watch-list
- README's "CLI Commands" table is **out of date** vs. the actual clap surface in
  `regin-cli/src/main.rs` (e.g. it lists `regin skill ...` but the verb is
  `regin task ...`, and omits `memory` / `ping`). Reconcile when touching docs.

## Session log
<!-- Append: ## YYYY-MM-DD — <slug> ... -->

### 2026-06-29 — FEAT-027: Portability verbs memory export/import (0.6.0)
- **FEAT-027 implemented and moved to done/.** Three new portability verbs:
  - `regin memory export <path>` — `VACUUM INTO` creates a consistent compact
    snapshot; stamps `exported_from` (hostname) and `exported_at` (timestamp)
    in the snapshot's `identity_meta`.
  - `regin memory import <path> [--merge]` — opens the snapshot, checks
    `identity_id` collision: different identity → refuse; same identity without
    `--merge` → refuse with hint; same identity with `--merge` → INSERT OR
    IGNORE all memories from the snapshot (18-column copy with
    `params_from_iter` since `params!` caps at 16).
  - `regin memory info` — surfaces identity_id, name, host, schema_version,
    memory_count, created_at.
  - Daemon dispatch + protocol Request/Response variants wired.
  - 250 total tests pass, clippy-clean.

### 2026-06-29 — FEAT-026: Vector/embedding recall (0.6.0)
- **FEAT-026 implemented and moved to done/.** Hybrid FTS5 + embedding recall:
  - `MimirClient::embedding()` → calls `/v1/embeddings`, returns `Vec<f32>`.
  - `cosine_similarity()` — deterministic unit tests with fixed vectors.
  - `hybrid_search_ranked()` — merges FTS5 BM25 candidates + cosine-similarity
    vector candidates, reranks by unified activation, reinforces hits.
  - `store_memory_embedding()` / `memories_pending_embedding()` — BLOB
    persistence and backfill plumbing.
  - Daemon: MemorySearch computes query embedding (best-effort) then calls
    `hybrid_search_ranked`; MemorySave/MemoryUpdate fire-and-forget embedding
    via `state_embed_memory`; `run_curation` backfills up to 10 per pass.
  - Config: `memory.embeddings.enabled` (default true), `memory.embeddings.model`
    (default "auto").
  - Graceful fallback: embedding failure or disabled → FTS-only
    `memory_search_ranked`, no crash.
  - 7 new tests covering cosine similarity, embedding persistence, pending
    detection, hybrid semantic match, FTS fallback, host scoping, reinforcement.
  - Critical design decision: MutexGuard must NOT be held across `.await` in
    `run_curation`; use block scoping (`{ let idb = ...; ... }`) to drop the
    guard before any `.await`.
  - 250 total tests pass, clippy-clean.

### 2026-06-29 — FEAT-025: Activation-ranked retrieval (0.6.0)
- **FEAT-025 implemented and moved to done/.** Replaced the old `LIKE`-based
  `memory_search` with FTS5 BM25 + activation reranking:
  - `memory_search_ranked()`: FTS5 MATCH candidate selection, activation score =
    f(BM25 * 10 + recency_score + retrieval_count*0.1 + trust_score*5 + strength*2).
  - Self-reinforcing: each returned hit bumps `retrieval_count` and `last_retrieved`.
  - Host-scoped: `host IS NULL OR host = ?` filters per-machine memories.
  - `context_memories()`: activation-ranked with pinned-first ordering and
    configurable budget, used in `build_context` for the system prompt.
  - Daemon: MemorySearch dispatch uses `memory_search_ranked`; `build_context` uses
    `context_memories` with budget=50 + hostname scope.
  - 5 new tests covering FTS matching, reinforcement, host scoping, pinned-first
    ordering, and budget. 231 total workspace tests pass, clippy-clean.

### 2026-06-29 — FEAT-024: Consolidation pipeline (Curator) (0.6.0)
- **FEAT-024 implemented and moved to done/.** Full Curator pipeline replacing the old
  simple reflection (FEAT-005/006) with DISC-017 tiered consolidation:
  - New types: `CuratorAction` (Add/Update/Delete/Noop), `CuratorProposal` (with
    `target_id`, `topic`, `tags`), `CuratorStats` in types.rs.
  - New identity_db functions: `memory_promote()` (medium→long at threshold),
    enhanced `memory_decay()` (medium decays faster, long only at low strength),
    `topic_ensure()`, `topic_update_summary()`, `topic_list()`,
    `transcript_unconsolidated()`, `curator_apply_proposal()` (handles all 4 actions).
  - New reflect.rs functions: `curation_prompt()` (now includes sessions),
    `parse_curator_proposals()`, `apply_curation()` (interference resolution),
    `gather_curation_inputs()`, `mark_consolidated()`,
    `post_curation_maintenance()` (promote + decay + prune).
  - Daemon: `run_curation()` replaces `run_reflection()`, releases DB lock around
    LLM call. `reflection_checker` now logs full CuratorStats.
  - Backward-compatible: `ReflectionProposal`, `apply_reflection()`, `reflect_once()`
    retained for simpler use cases.
  - 48 new tests across identity_db (14) + reflect (8) covering all actions, tiers,
    topics, transcripts, and maintenance. 226 total workspace tests pass, clippy-clean.

### 2026-06-28 — FEAT-022: migrate episodes + memories to identity.db (0.6.0)
- **FEAT-023 implemented and moved to done/.** Session archival + transcript capture.
  Extended `sessions` schema with `host`, `kind`, `title`, `message_count`,
  `token_count`, `state`, `transcript_text` (dropped `episode_id` — episode FK now
  goes `episodes.ref_id → sessions.id`). Added `session_open()`,
  `session_open_with_id()`, `transcript_append()`, `session_close()`, `session_list()`,
  `session_get()`, `hostname()` to identity_db.rs. Added `SessionRow`,
  `SessionWithTranscript`, `TranscriptMessage` types. Wired into daemon: `ChatNew`
  opens a session, `ChatSend` appends user/assistant messages and closes on completion
  with a title-based summary. 9 new tests (30 total identity_db tests). 206 workspace
  tests pass, clippy-clean.

- **FEAT-022 implemented and moved to done/.** All memory/episode CRUD functions
  mirrored in `identity_db.rs` (memory_list/search/save/update/delete, memory_list_for_repo,
  etc.) — 27 tests. Added `ref_id` column to identity.db episodes table. Wired identity_db
  into daemon: `AppState.identity_db`, startup init + migration, dispatch handlers,
  `build_context`, `run_reflection` all redirect to identity_db for memory/episode ops.
  reflect.rs uses `identity_db::*` instead of `db::*`. 21 identity_db tests + 5 regind
  dispatch tests + full workspace tests pass. Clippy-clean (pre-existing warnings only).

### 2026-06-28 — FEAT-021: identity.db store + schema bootstrap (0.6.0 foundation)
- **FEAT-021 implemented and moved to done/.** New `regin-core/src/identity_db.rs`
  module with `init_identity_db(path)`, `init_identity_schema(conn)`, and `meta_get()`.
  Full DISC-017 schema: `identity_meta` (key/value seeded on first bootstrap),
  episodic tier (`episodes` with `kind`/`host`/`importance`/`state`, `sessions`,
  `transcripts`), long-term tier (`topics` with hierarchy, `memories` with
  `topic_id`/`tier`/`host`/`embedding`/`trust_score`), FTS5 virtual tables
  (`memories_fts`, `transcripts_fts`) with `ai`/`ad`/`au` sync triggers, and 9
  indexes. Added `identity_db_path()` to config.rs; exposed `pub mod identity_db`
  in lib.rs. No Cargo.toml changes needed (FTS5 ships in bundled SQLite by default).
  11 unit tests covering idempotency, table/trigger/index creation, FTS sync, and
  file-backed open/reopen. 187 total workspace tests pass, clippy-clean (pre-existing
  clippy errors in llm.rs/db.rs/tools.rs are unchanged).

### 2026-06-19 — DISC-017 opened (portable identity vs machine apparatus)
- **New orthogonal axis (corrected with user):** *identity* (portable, travels with
  the agent) vs *machine apparatus* (local, rebuilt per box). Distinct from the
  operator/foreman plane axis. Identity = self-improving memory (archived sessions,
  documented transcripts, topic-based knowledge summaries, distilled semantic
  memory) in its **own SQLite DB**, copyable container→container. Machine apparatus
  = ITIL records (incident/change/problem/KPI), **audit results**, local
  tools/skills/docs, desired-state/filters/derived-checks — stays in `regin.db` +
  files on the box.
- **Audit is apparatus, NOT identity.** `operations/audit.md` + DISC-016 self-audit
  execute rules and raise ITIL incidents bound to the machine; nothing to do with
  self-improvement. Do not conflate.
- **"Hermes" = external reference project** (NousResearch hermes-agent), the
  *inspiration* for the memory design — not a regin subsystem. DISC-002/FEAT-006's
  internal "Hermes" nickname is loose; not rewritten (shipped tickets are immutable
  per audit rule). New subsystem name is a DISC-017 open question.
- **Current state violates the split:** DISC-002 → FEAT-005 (episodic) + FEAT-006
  (semantic reflection) put memory in the *same* `regin.db` as ITIL/runs/settings.
  DISC-017 reconsiders where that store lives AND extends it (sessions/transcripts/
  topic knowledge), which DISC-002 never covered.
- DISC-017 filed **open/pending** (decision via guided Q&A, like DISC-008/016).
  Open questions: boundary (where do chats/runs fall?), store shape (2nd SQLite vs
  files+index vs hybrid), topic-knowledge schema + consolidation pipeline,
  portability mechanics (export/import verb, identity id, relation to DISC-003 dwarf
  identity), machine-scoped knowledge, migration off `regin.db`, naming.

### 2026-06-19 — DISC-009 decided (three-lane remediation guardrail)
- **Q1 mechanism = Hybrid (H):** capability ceiling + declarative safe-action
  fast-path + LLM judgement within bounds.
- **Q2 posture = adaptive (earn autonomy):** start conservative; safe-lane
  graduates to auto-apply as change-success / autonomy KPIs prove trust — same
  earn-trust pattern as DISC-015 promotion, same KPI store.
- **Q3 safe-lane gate = rollback plan + dry-run:** auto-apply only if a concrete
  backout/undo is captured *before* applying (snapshot/backup/reversible) with
  bounded blast radius, plus dry-run where supported. Allowlist = fast-path of
  pre-blessed reversible ops.
- **Q4 ceiling = own operator ceiling + minimal static global red-lines.** Operator
  ceiling (editable) does day-to-day work; a tiny non-runtime-adjustable global
  red-line set protects the safety substrate (backups/snapshots/audit/KPI store),
  governance (service stop, escalation channel, kill-switch) and bars catastrophic
  host actions. Rationale: operator ceiling is editable + regin ingests logs
  (prompt-injection surface), so the global layer is defense-in-depth. Note: Q3's
  safe-lane *requires* the "never delete rollback/audit substrate" red-line.

### 2026-06-19 — DISC-015 decided (KPIs + promotion loop) + DISC-016 (self-audit) spawned
- **DISC-015 resolved via guided Q&A.** KPI framework reviewed/expanded first:
  reliability is a **constraint not a currency** (minimise cost s.t. reliability ≥
  floor); north-star = cost ↓ while **time-in-deviation** ↓; added **automation
  ratio** (the direct "senseful full automation" gauge), notice-filter savings,
  cost-avoided, MTTD/MTTR, recurrence, **promotion-error rate**, **autonomy ratio**.
  - Q1 objective = constrained; **all four KPI groups in v1** (schema defined now;
    promotion-error/autonomy KPIs report once their features exist).
  - Q2 surface = CSI summary in the login greeting (DISC-010) + a `regin metrics`
    command; KPIs in SQLite beside ITIL records.
  - Q3 promotion = **autonomous + audit trail**; criteria are **regin-owned and
    self-adapting**, grounded in *both* N-consistent+confidence and a statistical
    error-bound; governed by the promotion-error KPI, safety-netted by demotion.
  - Q5 notice filters = regin-managed, hand-editable **rule files** in a dedicated
    filters store (separate from `desired/`).
  - Q6 promoted checks = a **separate machine-managed derived-checks store** that
    references the to-be-state (not written into the human-authored structured
    layer); scheduler (DISC-013) runs them.
- **DISC-016 — periodic operator self-audit (NEW, from DISC-015 Q4).** Demotion is
  just one function: a regular (e.g. monthly) wide-lens CSI review that re-judges
  promoted checks, reviews KPI trends, tunes promotion criteria + notice filters,
  checks to-be-state drift, and files findings as ITIL artefacts (heavier actions
  via the DISC-009 approval gate). Immediate demotion on real-world
  contradiction/override stays in DISC-015. Cadence/scope/authority/cost-ceiling
  are its open questions.

### 2026-06-19 — DISC-008 resolved + DISC-015 (monitoring economics) spawned
- **DISC-008 (to-be state) closed via guided Q&A:**
  - **Three-layer target:** explicit markdown (intent) + structured assertions
    (machine-checkable) + implicit monitor-skill thresholds. They must agree —
    **deviation from target → incident**; **conflict *within* the target →
    problem** (the definition is ambiguous, needs a human).
  - **Stored as files, like skills** (not SQLite) — a scoped, deliberate exception
    to "all state in SQLite": desired-state is authored *content*, so it follows
    the skills precedent (`~/.config/regin/desired/` over a possible
    `/etc/regin/desired/`; `regind` is a per-user service). **Per-domain files**
    (`disk.md`, `services.md`, …), mapping 1:1 to the operator skill catalog
    (DISC-012).
  - **Deviation is LLM-judged, not raw events** — not every monitoring event is an
    incident; judging worth-against-intent is the point of the LLM.
- **DISC-015 — adaptive monitoring economics (NEW, spawned from DISC-008 Q4):** the
  ITIL **CSI** loop on regin's own monitoring. Guiding principle = **"senseful full
  automation"** (René's professor): automate fully but only where it makes sense.
  Two tiers (periodic LLM review ↔ cheap hourly deterministic checks); a
  **promotion loop** distils crystal-clear LLM verdicts into deterministic checks
  (land in DISC-008's structured layer; cadence per DISC-013); **notice filters**
  cut tokens before the LLM; reversible **demotion**; and **measurable** metrics
  (cost ↓ over time while reliability ↑, problems ↓). **"Senseful full automation"
  goes into regin's operator-plane system prompt** (baseline operator directive,
  cf. `regin-core/src/context.rs`).

### 2026-06-19 — operator model + operator/foreman plane split (MILESTONE-0.5.0)
- **Two planes (governing framing, near-settled — confirm in guided Q&A):**
  - **Operator plane** — regin as the autonomous operator of the *machine/
    container* it runs on. Governed by ITIL: a declared **to-be state**, and the
    loop deviation→**incident**→**change** (problems are the exception).
    `operator` is a distinct role from `foreman`.
  - **Foreman/repo-worker plane** — regin working *inside a repo* under that
    repo's own methodology (software ≠ marketing campaign), delegating to
    Claude/opencode or acting directly. ITIL does **not** apply here; repo
    methodology does. (To be discussed after the operator plane.)
- **Operator loop (converged with user):** to-be state (explicit markdown +
  implicit monitor thresholds) is the reference. Deviation → incident (always;
  incidents are by-definition solvable). The fix is usually a **change applied
  directly to the incident** (e.g. `/` full → delete temp files). A **problem**
  is raised only on (a) recurrence, or (b) a fix that is out-of-regin's-control
  or destructive. A problem carries hypotheses, may need long-run monitoring, and
  its real fix is a **change out of the problem** (e.g. adjust log rotation).
- **Three-lane risk guardrail == the autonomy policy:** each candidate change is
  judged safe-reversible (**auto-apply**) / uncertain-destructive
  (**`pending_approval`**, ask first) / out-of-control (**problem + escalate**).
- **Escalation routes by runtime mode:** supervisor over the bus if dvalin is
  reachable, else **parked for the human-at-login greeting** (`regin chat` opens
  with health status + the problems/changes it needs help with). Replaces an
  active push channel. Note: mode is currently inferred only from persona config
  and the escalation bridge has no offline fallback — both are gaps.
- Captured as **DISC-008** (to-be state), **DISC-009** (remediation + guardrail),
  **DISC-010** (mode-routed escalation + login greeting), **DISC-011** (ITIL model
  extensions: incident `blocked`+workaround, `change.problem_id`, change
  `pending_approval`, problem hypotheses; drop redundant `incidents.problem_id`).
  Still to open: DISC-012 operator skill catalog, DISC-013 scheduling+resilience,
  DISC-014 apk/rpm. All under MILESTONE-0.5.0; resolving open sub-questions via
  guided Q&A before minting FEATs.

### 2026-06-17 — methodology install
- Ported the dvalin V-Model methodology into this repo: `AGENTS.md`,
  `.repo/project/skills/` (canonical, verbatim), `.repo/project/audit/rules/`,
  an empty `.repo/project/issues/` ticket scaffold, and a regin-specific
  `.repo/project/profile.md`. No dvalin work-tickets were copied.

### 2026-06-17 — MILESTONE-0.2.0 planned (operations discipline)
- Verified items 1–6 of the operations request already exist in code (config in
  SQLite, `config list/get/set`, api_key in SQLite, `daemon.enabled` lingering
  systemd service, unrestricted tools). Only real gap: `ensure_daemon()` spawns a
  loose `regind` instead of registering the systemd service → BUG-001.
- Filed the plan as MILESTONE-0.2.0 with DISC-001 (ITIL ops agent), DISC-002
  (Hermes tiered+reflective memory), FEAT-001..007, BUG-001. User decisions:
  Hermes = self-reflective + tiered episodic; ITIL = SQLite records + methodology
  docs; approach = plan-as-milestone first, then one ticket = one PR.
- DISC-003 captures "regin as a dvalin dwarf" (future, not in 0.2.0): recommended
  A→B→C layering — JSON-API ticket escalation → ops executor plugin → dwarf-pool
  membership. Hard constraint: keep regin's LLM inside the executor (dvalin's
  no-LLM engine invariant). Closing DISC-003 spawns FEATs in BOTH repos.
- Nothing implemented yet — next step is to start the milestone, foundation
  tickets first (FEAT-002 data model, FEAT-005 episodic tier).

### 2026-06-19 — Identity plane finalized through 0.6.0 (DISC-017 resolved, DISC-018 added, FEAT-021..032 minted)
- **DISC-017 RESOLVED** (was logged open/pending above). Decision: portable
  `identity.db` (Variant A), 4-tier memory model (working/episodic/medium/long),
  topic-indexed long-term knowledge, activation-ranked FTS **+ vector** recall
  (scoped override of DISC-002's embedding deferral, memory-plane only), active
  decay + interference resolution; per-host `host` column travels with the identity
  but injects only on the matching host; **move-and-delete** migration off
  `regin.db`. Subsystem name = **memory plane**; verbs under `regin memory …`.
  Spawned **FEAT-021..027**.
- **DISC-018 NEW + resolved — decision plane** (`Persona / Mind / Soul / Body`):
  - **Sharp glossary (load-bearing):** Persona = *outward* identity (role/mask,
    FEAT-011); Mind = reasoning (plans/decides); Soul = *inner* identity
    (values-grounded conscience); Body = execution. Dropped the earlier "ego/false
    identity" overload on Mind — Persona is the mask, Soul is the character.
  - **Two modes:** `act` (`Mind → Body`, fast default) vs `deliberate`
    (`Mind ⇄ Soul → Body`: read-only plan, starved Soul votes, approved plan runs).
  - **Q1 trigger = risk-gated**, reusing DISC-009's blast-radius/reversibility
    judgement (+ Persona/urgency modifiers). **Q2 deadlock = Soul veto →
    default-deny + escalate** (FEAT-015/DISC-010). **Q3 grounding = values subset**.
    **Q4 = capture deliberations** (plan+vote+outcome) for calibration. **Q5 =
    principles derived seed + reflection-proposes + human-ratified** (keeps the
    conscience independent of the Mind).
  - **Values model = core + per-role overlay** (decided with user): persistent
    identity-core values in `identity.db` (portable) ∪ active Persona's overlay in
    `persona.toml` (swappable). The Soul votes from the union.
  - **Soul configurator (FEAT-030):** bundled value catalog drawn across human
    history/literature (Stoic/cardinal, theological, Confucian, chivalric,
    Enlightenment, Schwartz taxonomy, + agent-operational virtues); `regin soul …`
    CLI seeds the core charter; Persona→values derivation proposes a role-default
    set for human confirmation.
  - Spawned **FEAT-028..032**.
- **MILESTONE-0.6.0 — Identity plane** created (`status: planned`, depends_on 0.5.0),
  grouping the memory plane (021–027) + decision plane (028–032), with delivery
  order + exit criteria. **0.5.0 stays the single active milestone** (RULE-008).
- **Naming collisions avoided:** modes are `act`/`deliberate` (NOT "reflection
  mode" — `reflect.rs` = memory consolidation; `planning.rs` = org cadence).
- **Reminder honoured:** did NOT create a `ROADMAP.md` (standing rule — roadmap =
  the milestone files). Earlier in-session offer to render one was retracted.
- **Still user-gated (discuss-first, NOT auto-finalized):** the 0.5.0 operator
  capability backlog — DISC-009 (decided, to derive), DISC-015 (decided, to
  derive), and open/to-open DISC-008/010/011/012/013/014/016. These need user
  decisions before FEATs; out of scope for "till 0.6" identity-plane finalization.

### 2026-06-19 — Operator plane fully decided (DISC-011/010/016 resolved; DISC-012/013/014 opened + resolved)
- All operator-plane discoveries (DISC-008..016) are now **filed + decided**. MILESTONE
  -0.5.0 table updated to match (several were stale "open"). FEATs not yet minted —
  derive as the 0.5.0 capability batch.
- **DISC-011 (ITIL extensions):** `blocked` = first-class incident status + `workaround`;
  hypotheses = minimal structured rows (text + created→validating→confirmed/rejected);
  recurrence threshold = global default 3 + per-domain override in the to-be-state doc.
  Confirmed: `change.problem_id`, change `pending_approval` + approver/approved_at, drop
  redundant `incidents.problem_id` (keep join). → 2 features.
- **DISC-010 (escalation + greeting):** mode detection = Variant C (effective mode =
  configured target AND recent reachability). Login greeting = actionable items
  (pending-approval changes + problems) + one-line health (counts open incidents).
  **Critical push = opt-in, critical-only, OFF by default, IN v1** (ntfy/webhook/email)
  — user upgraded from my "defer". On bus recovery = auto-flush parked items,
  re-validated. Distinct from FEAT-015 (dev-plane ticket mint vs human decision/approval).
  → 5 features.
- **DISC-016 (periodic self-audit):** Variant A scheduled audit skill. Cadence =
  adaptive (monthly default). Scope = full CSI sweep (demotion, KPI review, criteria
  tuning, filter hygiene, to-be-state drift, coverage), modular per dependency. Output =
  report + ITIL artefacts + summary in `regin metrics`/login greeting. Authority = reuse
  DISC-009 lanes; to-be-state edits ALWAYS need approval. Cost = budgeted; skip/trim is a
  tracked event. → 2 features.
- **DISC-012 (operator skill catalog) — opened + resolved:** anatomy = structured bundle
  (monitor + to-be-state domain file + remediation playbook, fixes tagged for DISC-009
  lanes), one skill per domain. v1 scope = **broad ~12 domains** (disk, services, memory/
  load, logs, security-updates, certificates, backups, network, time-sync, users/auth,
  processes, firewall) — user chose broad over my core-8. Remediation = mixed per-domain
  (remediate disk/services/logs/time-sync/backups/security-updates; monitor-only +
  escalate the rest). Packaging = `regin-operator-skills` system package (3 existing
  report-only skills fold in), user-overridable. → 3 features. Distinct from DISC-007
  (org/role packaging).
- **DISC-013 (scheduling + resilience) — opened + resolved:** cadence = skill-declared
  default + user/config override (+ optional to-be-state per-domain tune); automatic
  jitter. LLM outage/over-budget = exponential backoff + **degrade to DISC-015
  deterministic checks** + self-incident if prolonged. Missed runs = coalesced run-once
  catch-up (staleness-checked). Watchdog = systemd lingering service + internal heartbeat
  + self-incident on repeated skill failure. → 4 features.
- **DISC-014 (apk/rpm) — opened + resolved:** add **both** rpm + apk; build via **nfpm**
  (one config → deb+rpm+apk, replacing the deb-only FEAT-020 recipe); land **in 0.5.0**.
  Consequence: **profile §7 deb-only → deb/rpm/apk first-class** (gate removed); FEAT-020
  reworked to nfpm. → 3 features (nfpm packaging, per-format install PITs, profile §7
  update).
- **Next:** mint the 0.5.0 operator-capability FEATs from DISC-009/010/011/012/013/014/
  015/016 (currently all decided, features listed but not yet ticketed), then sequence
  0.5.0 delivery. profile.md §7 still says deb-only — update when FEAT work starts.

### 2026-06-19 — MILESTONE-0.5.0 fully planned (FEAT-033..059 minted)
- Minted the **operator-capability FEATs (FEAT-033..055)** from the decided operator
  discoveries, and the **release-readiness FEATs (FEAT-056..059)** for the delivery
  prerequisites (install script, wiki, coverage CI gate, release automation).
- **FEAT-020 superseded by FEAT-053** (nfpm folds the deb recipe into one config that
  also builds rpm + apk). Noted in both files + the milestone.
- MILESTONE-0.5.0 reworked: docs/packaging + operator-capability + release-readiness
  issue tables, dependency-ordered delivery plan (foundation→evaluation→skills→
  remediation loop→optimisation→packaging→release), operator-loop exit criteria,
  prerequisites table now all "filed". Intro no longer says "pending discovery".
- 0.5.0 is **fully planned** (every DISC decided, every FEAT minted, no "(file at
  planning)" gaps). Remaining = implementation only (one ticket → one PR, V-Model
  phases). Kept as a single milestone (user: split "does not matter").
- Still pending at IMPLEMENTATION time (not planning gaps): update profile.md §7 to
  deb/rpm/apk (FEAT-053); the milestone is large (~29 FEATs incl. release readiness) —
  split remains an option if desired.

### 2026-07-12 — FEAT-070: CLI transport seam + render/logic split (0.6.0 coverage)
- **FEAT-070 implemented and moved to done/.** New `regin-cli/src/transport.rs`:
  a `Transport` trait (`request` for single round-trips, `request_stream` for
  multi-event exchanges with a live per-event callback) with `SocketTransport`
  (the real Unix-socket implementation — `connect_daemon`/`ensure_daemon`/
  `send_req`/`read_resp` and the daemon-auto-start plumbing all moved here
  verbatim) and a `#[cfg(test)] FakeTransport` (canned replies, records sent
  `Request`s). New `regin-cli/src/render.rs`: ~30 pure `Response -> String`
  render functions (task/memory/ITIL/desired/metrics/audit/etc. listings) plus
  streaming-event helpers (`apply_chat_event`, `render_tool_call`,
  `render_tool_result`, `render_task_result`).
- All ~40 daemon-calling `cmd_*` functions in `main.rs` now take `&impl
  Transport` and call a render fn instead of printing inline. `cmd_chat` and
  `cmd_task_exec` (the two streaming/interactive commands) route through
  `request_stream`'s callback so production output is still live
  (event-by-event) while `FakeTransport` can replay a canned sequence in
  tests. Local-only commands (bus/persona/deputy/skill-install, no daemon
  round-trip) were left as-is.
- **Deliberate simplification:** dropped the old per-segment ANSI colouring
  inside render fns in favour of plain `String` output (colour is now applied
  as a single wrap around the whole rendered block at the call site) — makes
  every render fn a trivial, comparable `String` in tests. Documented in the
  PR; not expected to be missed (the CLI is still coloured, just less
  granularly).
- 64 new tests (66 total in `regin-cli`, up from 2): every render fn has a
  unit test, every daemon-calling `cmd_*` is exercised via `FakeTransport`
  (happy path + an error `Response`), and a `Cli::try_parse_from` sweep covers
  the full command/subcommand/flag surface plus a required-arg-rejection
  check. Full workspace (`cargo build/test/clippy --workspace`) stays green;
  `regin-cli` itself is clippy-clean.
- No `cargo-llvm-cov` available in this sandbox to print an exact percentage —
  precise measurement + the `COVERAGE_MIN` gate ramp is FEAT-075's job once
  072–074 land too.
- Sets up FEAT-071 (injectable `LlmClient`) next, then the decision-plane
  FEATs (028→030→029→032→031), per the milestone's suggested delivery order.

### 2026-07-12 — FEAT-071: Injectable LLM client (0.6.0 coverage)
- **FEAT-071 implemented and moved to done/.** New `LlmClient` trait in
  `regin-core/src/llm.rs` (`#[async_trait]`, object-safe — `dyn LlmClient`)
  covering the surface actually used across the codebase: `chat_turn`,
  `embedding`, and `chat_completion` (default-impl'd in terms of
  `chat_turn`). `chat_completion_stream`/`stream_messages` stayed inherent
  `MimirClient`-only methods — grepped the whole workspace and found zero
  call sites for them anywhere (SSE streaming isn't actually wired up; the
  daemon "streams" by sending the final text as one `StreamChunk` — see
  `agentic_chat`), so they weren't worth trait-ifying.
- `impl LlmClient for MimirClient` delegates to the existing inherent
  methods (`self.chat_turn(...)` inside the impl still resolves to the
  inherent method — Rust always prefers inherent over trait methods for a
  concrete receiver — confirmed not recursive). Zero behaviour change for
  the concrete type.
- **`FakeLlm`** (also in `llm.rs`, *not* `#[cfg(test)]`-gated since `regind`
  — a different crate — needs it in its own tests): independent FIFO queues
  for `chat_turn`/`chat_completion`/`embedding` replies.
- **`AppState.llm_client()`** (regind) changed from constructing a fresh
  `MimirClient` from live config on every call to returning
  `Arc<dyn LlmClient>` — but **kept the fresh-config-read behavior** for the
  production path (a new `llm_override: Option<Arc<dyn LlmClient>>` field on
  `AppState`, `None` in production, short-circuits straight to the injected
  client in tests). Deliberate: a naive "construct once at startup" design
  would have silently broken live `regin config set mimir.*` reconfiguration
  (no daemon restart needed today) — documented in a doc-comment so nobody
  "simplifies" this into `AppState` holding a bare `Arc<dyn LlmClient>` field
  set once at construction.
- `reflect::curate_once`/`reflect::reflect_once`/`skills::run_skill` signatures
  changed `client: &MimirClient` → `&dyn LlmClient` (mechanical; not currently
  called from `regind` — it reimplements curation/reflection inline in
  `run_curation` — so this is forward hygiene, not a behavior change).
- New `regind::dispatch_tests`: `chat_send_uses_the_injected_llm_client` drives
  `Request::ChatNew` → `Request::ChatSend` through the *real* `dispatch()` with
  a `FakeLlm` queued reply and asserts the `StreamChunk`/`StreamDone` carries
  it — no network. `TaskExec` shares the same `agentic_chat` LLM loop (via
  `exec_skill_agentic`), so this one test covers the LLM-dependent code path
  both commands rely on; didn't additionally stand up a temp skills dir to
  drive `TaskExec` literally, since it'd exercise the identical `chat_turn`
  seam for marginal extra coverage.
- Added `async-trait = "0.1"` as a new workspace dependency — required for
  `dyn LlmClient` (native async-fn-in-trait isn't object-safe without it, and
  `AppState` holding a boxed/`Arc` trait object was explicit in the ticket).
- 8 new tests (regin-core: +5 llm_client_trait_tests; regind: +3 dispatch
  tests). Full workspace build/test/clippy stays green.
- Next: the decision-plane FEATs, starting with FEAT-028 (dual-mode agent
  loop), per the milestone's suggested order.

### 2026-07-12 — FEAT-028: Dual-mode agent loop (act vs deliberate) (0.6.0 decision plane)
- **FEAT-028 implemented and moved to done/.** New `regin-core/src/decision.rs`
  — pure-ish engine, same pattern as `remediation.rs`/`safelane.rs` (both of
  which are *also* not wired into `regind`'s live loop yet — confirmed by
  grep, zero call sites in `regind/src/main.rs`). This ticket follows suit
  deliberately: **not wired into `agentic_chat`**. Reasoning captured in the
  module doc comment so it isn't mistaken for an oversight — wiring a
  stub-that-always-approves `SoulGate` into the live chat loop would add a
  new production code path for zero behavioural benefit until FEAT-029 lands
  the real gate. `act` mode (today's `chat_turn` path) is therefore unchanged
  *by construction*, satisfying acceptance criterion 5 without a regression
  test against `agentic_chat` itself.
- **Mode selection:** `ContemplatedAction { reversible, destructive,
  outward_facing, urgent }` + a pluggable `RiskClassifier` trait.
  `DefaultRiskClassifier` mirrors DISC-009's blast-radius/reversibility
  judgement (irreversible/destructive/outward → deliberate; urgency
  overrides to act). `select_mode()` lets a `Persona.default_mode` override
  (new optional field, `"act"`|`"deliberate"`, validated) win outright over
  the classifier.
- **Deliberate pipeline:** `Planner` (async, produces a read-only `Plan` —
  `intent_summary`/`steps`/`intended_tool_calls`, zero side effects) → 
  `SoulGate` (`Approve`/`Revise`/`Veto` + one-line reaction) →
  `Executor` (only reached on `Approve`). `run_deliberate()` loops
  plan→gate up to `max_rounds` (config: `decision.deliberate.max_rounds`,
  default 3; `decision.default_mode`, default `act`), feeding `Revise`'s
  reaction back into the next planning round, and returns
  `DeniedAndEscalated { reason }` on `Veto` or exhausted rounds — the actual
  bus/dvalin escalation I/O (FEAT-015) is the *caller's* job when this gets
  wired up; this module signals it, doesn't perform it.
- `PassthroughSoulGate` (always approves) is the explicit stand-in for
  FEAT-029 — lets FEAT-028 land and be fully tested without a forward
  dependency on the ticket that hasn't been built yet.
- 14 new tests (regin-core 243→257): mode selection with both the real
  classifier and a `FakeClassifier` (acceptance criterion 1), a spy executor
  proving zero executions during planning/veto and exactly-one on approval
  (criteria 2–3), `max_rounds` + persona-override enforcement (criterion 4),
  a `Revise`-feedback round-trip test, and the `Persona.default_mode`
  field's own round-trip/validation tests.
- Full workspace build/test/clippy stays green (fixed one new
  `collapsible_if` clippy lint in `persona.rs`'s validation — everything else
  was pre-existing, untouched).
- Next: FEAT-030 (Soul configurator + value catalog), per the milestone's
  suggested order — lands before FEAT-029 so the real Soul gate has a value
  catalog to vote from when it's built.

### 2026-07-12 — FEAT-030: Soul configurator + value catalog (0.6.0 decision plane)
- **FEAT-030 implemented and moved to done/.** New `regin-core/assets/values.toml`
  (checked-in, versioned, `include_str!`-embedded) — ~37 entries across cardinal/
  Stoic, theological, Confucian, Aristotelian, chivalric, Enlightenment, Schwartz,
  and agent-operational traditions, each with `id`/`name`/`description`/`tradition`.
  New `regin-core/src/soul.rs`: `catalog()`/`find()` (parsed once via `OnceLock`),
  `role_default_values()` (built-in map for cfo/dev-lead/operator/security/foreman/
  auditor + a generic agent-operational-virtue fallback for unknown roles — the
  ticket's "LLM-assisted suggestion for novel roles" was scoped out: the acceptance
  criteria only require a *known* Persona's derive to work, and the deterministic
  fallback is instant/testable vs. a network call for marginal benefit),
  `grounding_union()` (core ∪ overlay, deduplicated, order-stable), and the
  privileged charter read/write path (`charter_seed`/`charter_core_ids`/
  `charter_remove`).
- **Core-charter immutability (acceptance criterion 4) — the trickiest part.**
  `identity_db::memory_update`/`memory_delete`/`curator_apply_proposal` had *zero*
  existing protection for `pinned` or any category — a real latent gap. Added a
  `PRINCIPLE_CATEGORY` guard: those three functions now refuse (Update/Delete →
  `Err`; the curator's Add/Update/Delete → silently `Ok(false)`, consistent with its
  existing "malformed proposal" convention) whenever the target row's category is
  `"principle"`. `soul::charter_seed`/`charter_remove` are the *only* privileged
  bypass — reachable solely from `regin soul charter`. No schema migration: the
  value id is encoded as a `"{id}: ..."` prefix in the memory's `content` (the
  `memories` table has no dedicated column for it, and adding one for a feature
  this narrow wasn't worth a migration).
- **Persona** gains two new optional, backward-compatible fields:
  `values: Vec<String>` (the per-role overlay this ticket needed) — `default_mode`
  was already added by FEAT-028.
- **Full protocol + CLI wiring** (not scoped out, unlike FEAT-028's deliberate
  pipeline): `Request`/`Response` variants (`SoulValuesList/Show`,
  `SoulCharterShow/Derive/Confirm/Remove`), `regind` dispatch arms (careful to
  scope every `identity_db` `MutexGuard` in a block *before* the `send(...).await`
  — the exact bug class flagged in FEAT-024's notes; caught it via a real compile
  error, "future cannot be sent between threads safely", not by memory), and a full
  `regin soul values list|show` / `regin soul charter show|derive|set|remove` CLI
  surface built on FEAT-070's `Transport`/render pattern.
- 34 new tests total: regin-core +17 (soul.rs catalog/derivation/union/charter
  round-trip, identity_db principle-guard enforcement, persona `values` overlay);
  regin-cli +8 (render fns + `FakeTransport`-driven `cmd_soul_*`, `Cli::try_parse_from`
  coverage for the whole `regin soul` tree). Full workspace build/test/clippy green.
- Next: FEAT-029 (the Soul gate itself) — now has both FEAT-028's pipeline
  (`SoulGate` trait, `PassthroughSoulGate` stub to replace) and FEAT-030's value
  catalog + grounding union to vote from.

### 2026-07-12 — FEAT-029: The Soul gate (values-grounded vote + veto) (0.6.0 decision plane)
- **FEAT-029 implemented and moved to done/.** Landed in `decision.rs` (not a
  new module) — it's the real implementation of the `SoulGate` trait FEAT-028
  already defined, so it belongs where that trait and `run_deliberate` live.
- **`SoulGate::evaluate` became `async fn ... -> Result<...>`** (was sync,
  infallible) — a real LLM call is inherently async/fallible. Updated
  `PassthroughSoulGate` and `run_deliberate`'s call site (`.await?`)
  accordingly; this is expected ticket-to-ticket evolution of FEAT-028's
  trait, not a mistake — the trait was always going to need this once a real
  gate showed up.
- **`LlmSoulGate`**: builds exactly two messages — a fixed system prompt
  ("you are the conscience... respond with ONLY a JSON object") and a user
  turn containing **only** `Plan.intent_summary` + the resolved values
  grounding (names/descriptions looked up from FEAT-030's catalog). Verified
  by a `SpyLlm` test double that records every message sent and asserts a
  plan's `steps`, `intended_tool_calls`, and tool names never appear in it —
  acceptance criterion 1, actually enforced by a test rather than just by
  code review.
- **Verdict resolution**: `veto` always wins; `approve` passes only at
  `confidence >= decision.deliberate.confidence_threshold` (new setting,
  default 0.7) — below-threshold `approve` *and* `revise` both resolve to
  `SoulVerdict::Revise`. Response parsing tolerates prose wrapped around the
  JSON object (LLMs don't reliably honour "ONLY JSON" — same lenient
  `{...}`-extraction pattern `reflect.rs`'s curator parsing already uses) and
  a genuinely malformed response is a hard `Err`, not a silent approve.
- **`SoulVote` + `VoteRecorder` trait + `NullVoteRecorder` stub** — same
  "define the seam, stub the sink" pattern as FEAT-028's `PassthroughSoulGate`.
  FEAT-032 ("deliberation capture") owns turning this into durable storage;
  FEAT-029's job was making sure every vote (`plan_id`, `confidence`,
  `verdict`, `gut_reaction`) is *produced* and handed to *something* —
  verified with a `SpyRecorder` capturing all 3 votes across a 3-round
  `max_rounds` exhaustion.
- `run_deliberate` itself needed **no changes** for criteria 3/4 (veto /
  max-rounds → `DeniedAndEscalated`) — that logic was already correct from
  FEAT-028; this ticket's tests just exercise it end-to-end through the real
  gate instead of a canned `FixedVerdictSoul`.
- Added `decision.deliberate.confidence_threshold` (default `0.7`) to
  `config::SETTINGS`.
- 9 new tests (regin-core 274→283), all in `decision.rs`'s existing test
  module: the prompt-starvation spy (criterion 1), threshold resolution both
  sides (criterion 2), veto both directly and through the full pipeline
  (criterion 3), max-rounds exhaustion with a scripted `LlmClient` plus vote
  capture (criteria 4–5), prose-tolerant parsing, and malformed-response
  error handling. Full workspace build/test/clippy stays green.
- Like FEAT-028, **`LlmSoulGate` is not wired into `regind`'s live chat
  loop** — no caller in `agentic_chat` constructs a `Planner`/`Executor`/
  `LlmSoulGate` yet. That live-loop integration is still a future ticket;
  this milestone's remaining decision-plane FEATs (031 principle
  derivation, 032 deliberation capture) are about deepening the engine
  further, not that integration.
- Next: FEAT-032 (deliberation capture — the real `VoteRecorder`), then
  FEAT-031 (principle derivation & ratification), per the milestone's
  suggested order.

### 2026-07-12 — FEAT-032: Deliberation capture (0.6.0 decision plane)
- **FEAT-032 implemented and moved to done/.** Landed in `decision.rs` and
  `identity_db.rs` — turns FEAT-028's `run_deliberate` and FEAT-029's Soul
  vote into durable `deliberation` episodes, closing the capture-and-learn
  loop DISC-018 Q4 calls for.
- **`DeliberationSink` trait + `NullDeliberationSink`/`IdentityDbSink`** —
  same "define the seam, stub the sink, then a real impl" pattern used
  throughout this milestone. `IdentityDbSink` wraps `Arc<Mutex<Connection>>`
  (owned, not borrowed) so it satisfies `Send + Sync` despite
  `rusqlite::Connection` not being `Sync` on its own — mirrors `AppState`'s
  own locking pattern in `regind`. `capture()` writes one `identity_db`
  episode (kind = `"deliberation"`, `ref_id` = plan id, `detail` = JSON
  `DeliberationRecord`).
- **Exactly one episode per completed deliberation, not one per revise
  round.** `run_deliberate` now tracks `last_round: Option<(Plan,
  SoulEvaluation)>` across the loop and calls a single
  `capture_best_effort()` at each of its three exit points (Approve, Veto,
  max-rounds exhaustion) — verified directly with a multi-round test
  (`RevisesOnceSoul`) asserting the episode count stays at 1 even after a
  revise round runs first.
- **`Disposition` (`Executed`/`Denied`/`Escalated`) and `Outcome`
  (`Success`/`Failure`/`RolledBack`)** — disposition is set at capture time
  from which loop-exit branch fired; outcome starts `None` and is
  back-filled later via `deliberation_backfill_outcome()`, which reads the
  episode's JSON detail, patches in `outcome`/`outcome_ref_id`, and writes it
  back — errors if the episode id doesn't exist (no silent no-op).
- **Fail-safe capture**: `capture_best_effort()` logs (`tracing::warn!`) and
  swallows any `DeliberationSink::capture` error rather than propagating it
  — a `FailingSink` test double confirms `run_deliberate` still returns the
  correct `DeliberateOutcome` even when every capture call errors.
- **Consolidation query path**: added `identity_db::episodes_by_kind` (plus
  `episode_detail`/`episode_set_detail` helpers) and a thin
  `decision::deliberation_episodes()` wrapper so FEAT-024's Curator (and,
  later, FEAT-031's principle derivation) can pull `deliberation` episodes
  without reaching into `identity_db` internals directly.
- 9 new tests (regin-core 283→292 across the two touched files), covering
  all 5 acceptance criteria: exactly-one-episode capture (single-round and
  multi-round), disposition correctness for all three dispositions, outcome
  back-fill (success path + unknown-episode error path), capture-failure
  non-blocking, and kind-scoped querying. `SoulGate::evaluate`'s return type
  changed from a `(SoulVerdict, String)` tuple to a `SoulEvaluation` struct
  (`verdict`, `reaction`, `confidence`, `raw_verdict`) so `capture_best_effort`
  has the full vote to record, not just the resolved verdict — updated all
  existing test doubles and call sites accordingly. Full workspace
  build/test/clippy stays green (292 regin-core tests, 0 new clippy warnings
  — the 24 pre-existing warnings on this branch are all in unrelated code).
- Like FEAT-028/029, capture is exercised entirely through `run_deliberate`
  in tests — no caller in `regind`'s live chat loop constructs a
  `DeliberationSink` yet; that live-loop wiring remains future work, not a
  gap in this ticket's scope.
- Next: FEAT-031 (principle derivation & ratification) — now has both
  FEAT-024's consolidation pipeline and FEAT-032's queryable `deliberation`
  episodes to derive principles from.
