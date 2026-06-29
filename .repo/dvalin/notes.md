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
