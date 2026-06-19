---
id: BUG-003
type: bug
priority: medium
complexity: XS
estimate_tokens: 10k-25k
estimate_time: 15-30min
phase: open
status: open
milestone: 0.5.0
---

# BUG-003 — In-code clap help cites retired context.md files (FEAT-008)

## Description
**As a** user running `regin chat --help`
**I want** the help text to describe how context is actually loaded
**So that** I am not pointed at files regin no longer reads

The `Chat` command's doc comment in `regin-cli/src/main.rs` still says it
"loads context from `~/.config/regin/context.md` and `.repo/regin/context.md`".
That mechanism was retired by FEAT-008: per-repo context/memories/skills now
live in regin's XDG store (SQLite), keyed by the repo's filesystem path, and are
managed via `regin context ...`. The help text is misleading.

## Implementation
- Update the `Chat` doc comment (and any sibling help/`after_help`) to describe
  the XDG per-repo store keyed by repo path, and point at `regin context`.
- Grep for other lingering `context.md` / `.repo/regin/` references in CLI help
  and fix them in the same pass.

## Acceptance Criteria
1. `regin chat --help` no longer references `~/.config/regin/context.md` or
   `.repo/regin/context.md`.
2. Help text accurately describes the XDG-store, repo-path-keyed context model
   and the `regin context` verb.
3. No other CLI help string references the retired in-repo context files.
