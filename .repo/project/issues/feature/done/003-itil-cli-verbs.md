---
id: FEAT-003
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 60-120min
phase: done
status: done
depends_on: FEAT-002
spawned_from: DISC-001
---

# ITIL CLI verb families (incident / change / problem)

## Description
**As an** operator
**I want** `regin incident`, `regin change`, and `regin problem` verb families
**So that** I can open, view, work, and close operational records from the CLI.

Thin client over the daemon, matching the existing CLI↔`regind` protocol style
(`Request`/`Response` over the Unix socket).

## Implementation
- Extend `protocol.rs` with request/response variants for the three families.
- Daemon handlers in `regind` call the FEAT-002 `db` layer.
- CLI subcommands (clap), grouped and documented like the existing `task` /
  `memory` families:
  - `incident open <title> [--severity] [--desc]` · `list [--status]` ·
    `show <id>` · `update <id> [--status] [--note]` · `resolve <id> <resolution>` ·
    `close <id>`
  - `change record <title> [--incident <id>] [--before] [--after]` ·
    `list` · `show <id>` · `apply <id>` · `close <id>`
  - `problem open <title> [--desc]` · `list` · `show <id>` ·
    `link <problem-id> <incident-id>` · `known-error <id> <root-cause>` ·
    `close <id>`
- Human-readable, colourised output consistent with current commands; stdout for
  data, stderr for logs.

## Acceptance Criteria
1. Each verb family supports its create/list/show/update/close lifecycle end to
   end against a live daemon.
2. `incident open … --severity high` persists and is visible in `incident list`
   and `show`.
3. `problem link` and `change record --incident` correctly associate records.
4. Help text is grouped and documented (long_about/after_help) like `task`.
5. Protocol round-trip covered by unit tests; manual-path documented in the PR.
