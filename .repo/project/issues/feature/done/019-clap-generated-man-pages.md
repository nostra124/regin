---
id: FEAT-019
type: feature
priority: high
complexity: M
estimate_tokens: 30k-70k
estimate_time: 45-90min
phase: open
status: open
milestone: 0.5.0
---

# FEAT-019 — Man pages generated from the clap surface

## Description
**As a** packager / sysadmin
**I want** complete, accurate man pages that cannot drift from the CLI
**So that** `man regin` documents every command the binary actually has

The hand-maintained `man/regin.1` is drifted: its SYNOPSIS covers only
`chat, task, runs, config, memory, ping` and omits the ~10 verbs added since —
`incident, change, problem, context, bus, persona, meeting, plan, foreman,
deputy` (and the `skill` package manager is only lightly covered). The dates and
copyright still say 2025. `man/regind.1` has the same hand-maintained risk.
Because they are written by hand, every CLI change re-introduces drift.

## Implementation
- Generate man pages from the clap definition (e.g. `clap_mangen`), most likely
  via a `build.rs` or a small `xtask`/`regin man` generator, so the source of
  truth is `regin-cli/src/main.rs`.
- Cover all top-level verbs and their subcommands; include a DESCRIPTION,
  ENVIRONMENT (e.g. `REGIN_PERSONA`, `REGIN_PROCESS_OWNER`, `REGIN_CAO`), FILES
  (XDG store / SQLite db), and SEE ALSO (`regind(1)`).
- Generate `regind.1` from the daemon's clap surface too.
- Wire generation + install path into FEAT-020 packaging so the package ships
  current man pages.
- Decide (Design phase) whether generated pages are committed or built — keeping
  RULE-011 (no build artifacts in git) in mind.

## Acceptance Criteria
1. `man/regin.1` documents every top-level verb present in the clap surface; a
   test/CI check fails if a verb exists with no man coverage.
2. `regind.1` documents the daemon's flags and behaviour.
3. Man pages carry the correct version and a current date, derived, not hard-coded.
4. The packaging recipe (FEAT-020) installs the generated man pages.
