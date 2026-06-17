# RULE-012 — Documentation coverage and currency

scope: full
severity: block

## Rule

Every shipped command, flag, and workflow must be documented in the
appropriate layer. Stale, missing, or misdirected documentation is a
block because it makes the project unusable without reading source code.

## Documentation layers

| Layer | Audience | Location | Covers |
|---|---|---|---|
| **README.md** | First-time visitors, evaluators | repo root | What it is, install, quickstart, 3-command example |
| **Man page** (`dvalin.1`) | End users | `docs/man/dvalin.1` | Every subcommand, every flag, exit codes, environment variables, examples |
| **docs/architecture.md** | Developers, architects | `docs/architecture.md` | Design principles, module map, data flow, extension points |
| **docs/operations.md** | Admins, operators | `docs/operations.md` | Install, upgrade, backup, container runtime requirements, credential model |
| **AGENTS.md** | Coding agents (dwarfs) | repo root | Agent bootstrap, methodology gateway, ordered reading list |

## README.md requirements

- [ ] One-paragraph description of what dvalin does and who it is for
- [ ] Prerequisites (Rust, cargo, podman, bats)
- [ ] Install instructions (`./configure && make && make install`)
- [ ] Quickstart: the 3 commands a new user runs first
- [ ] Link to man page or `dvalin --help` for full reference
- [ ] Current version visible or linked

## Man page (`docs/man/dvalin.1`) requirements

- [ ] Exists and is installed by `make install`
- [ ] `SYNOPSIS` section lists all subcommands
- [ ] Every subcommand has its own `DESCRIPTION` paragraph
- [ ] Every flag documented with type, default, and effect
- [ ] `EXIT STATUS` section: 0 = success, 1 = failure, specific meanings
- [ ] `ENVIRONMENT` section: all env vars dvalin reads or writes
- [ ] `FILES` section: `.repo/dvalin/`, `AGENTS.md`, milestone files, logs
- [ ] At least 3 `EXAMPLES` covering the most common workflows
- [ ] `SEE ALSO` references related commands
- [ ] `VERSION` / `DATE` in the header matches `Cargo.toml`

## docs/architecture.md requirements

- [ ] Core design principle stated: dvalin is rule/workflow-based, no LLM
- [ ] All dwarfs execute over containers — rationale explained
- [ ] Module map: one paragraph per `src/*.rs` file stating its purpose
- [ ] Data flow diagram or description: user → dvalin → dwarf → project
- [ ] Issue type overview: FEAT / BUG / DISC / AUDT and their phase models
- [ ] Kanban / TUI design intent (even if not yet implemented)
- [ ] Extension points: how to add a new subcommand, a new check, a new rule

## docs/operations.md requirements

- [ ] System requirements: OS, Rust toolchain version, container runtime
- [ ] Full install procedure (from source and from package)
- [ ] Credential model: what secrets are needed and how they are passed
- [ ] Container runtime: podman only, never docker — why
- [ ] Upgrade procedure: `dvalin upgrade`, version bumps
- [ ] Backup and restore
- [ ] Log locations and retention policy

## Pass criteria

- README.md is more than a title and tagline.
- `docs/man/dvalin.1` exists, installs, and covers every subcommand.
- `docs/architecture.md` and `docs/operations.md` both exist and are
  non-empty with the required sections.
- No subcommand in `src/cli.rs` lacks documentation in the man page.
- Version in man page header matches `Cargo.toml`.

## Fail criteria

- README.md is a stub (one line or only a title).
- Man page absent or not installed by `make install`.
- Any subcommand added since the last release is missing from the man page.
- `docs/architecture.md` or `docs/operations.md` absent.
- Any required section listed above is missing or contains only a placeholder.
- Version in man page differs from `Cargo.toml`.

## Audit instruction

1. `cat README.md` — verify it has description, install, quickstart.
2. `man docs/man/dvalin.1` or `groff -man docs/man/dvalin.1 | less` —
   walk every subcommand against `dvalin --help` output.
3. Compare `grep -E '^\s+(///|#\[command)' src/cli.rs` against man page
   SYNOPSIS — every subcommand listed in cli.rs must appear in the man page.
4. Check version: `grep 'version =' Cargo.toml` vs man page `.TH` date/version.
5. Verify `make install` installs the man page to `$(mandir)/man1/dvalin.1`.
6. Read `docs/architecture.md` and `docs/operations.md` section by section
   against the requirements above.
