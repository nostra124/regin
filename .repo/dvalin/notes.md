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
