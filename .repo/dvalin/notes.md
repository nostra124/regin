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
