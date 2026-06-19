---
id: BUG-002
type: bug
priority: high
complexity: S
estimate_tokens: 15k-40k
estimate_time: 20-45min
phase: open
status: open
milestone: 0.5.0
---

# BUG-002 — README CLI/config docs drifted from the clap surface

## Description
**As a** user reading the README
**I want** the documented commands and configuration to match the real CLI
**So that** I can actually run regin without hitting commands that don't exist

The README has drifted badly from the shipped `regin-cli` clap surface:

- The **"CLI Commands" table appears twice**, and both copies are stale: they
  list `regin skill list/run/show` (the verb is now `regin task ...`; `skill`
  is the skill-*package* manager) and omit every verb added since 0.2.0 —
  `memory`, `ping`, `incident`, `change`, `problem`, `context`, `bus`,
  `persona`, `meeting`, `plan`, `foreman`, `deputy`.
- The **Configuration** section describes a `~/.config/regin/config.toml` file
  and `nanogpt_*` TOML keys, but the system is **config-free**: all settings
  live in SQLite and are managed via `regin config set/get/list` (keys are
  `nanogpt.api_key`, `nanogpt.model`, `nanogpt.base_url`, `daemon.enabled`).
- The **Daemon** section shows a `--config <PATH>` flag that no longer matches
  the daemon's real flags.

## Implementation
- Replace both CLI-command tables with a single accurate table generated from /
  checked against the clap surface in `regin-cli/src/main.rs`.
- Rewrite the Configuration section for the SQLite/`config set` model; remove the
  `config.toml` example.
- Fix the daemon section's flags.
- Cross-check against FEAT-019 (man pages) so README and man stay consistent.

## Acceptance Criteria
1. Every command shown in the README exists in the clap surface; no command in
   the clap surface that a user needs is omitted from the overview.
2. No reference to `config.toml` or `nanogpt_*` TOML keys remains; configuration
   is documented as SQLite via `regin config set/get/list`.
3. The CLI-commands table appears exactly once.
4. Daemon flags shown match `regind`'s actual flags.
