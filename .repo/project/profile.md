# regin — project profile

## 1. What this project does

`regin` is an AI-powered Linux server administration agent written in Rust.
Named after the dwarf smith from the Völsunga saga, it provides an LLM-backed
agent with real tool access (shell, file I/O, web search) for server
monitoring, auditing, and operations tasks.

It has three cooperating parts that share one core library:

1. **regind (daemon)** — a per-user background service that holds the LLM
   client, the SQLite store, and the scheduler. It serves the CLI over a Unix
   socket and runs scheduled *tasks* (skills) on their cadence.

2. **regin (CLI)** — a thin client. It auto-starts the daemon on first use and
   talks to it over the socket: interactive chat, task management, config,
   and long-term memory.

3. **regin-core (library)** — shared crate: config, database, LLM client,
   skills engine, tool definitions, the request/response protocol, and shared
   types. Both binaries depend on it; no logic is duplicated across them.

The agent loop is LLM-driven with tool calls executed locally: `bash`,
`read_file`, `write_file`, `edit_file`, and `web_search` (DuckDuckGo).

## 2. Scope

### In scope
- Interactive streaming chat with full tool access (`regin chat`)
- Markdown-defined **tasks** (skills): list, show, run-once, schedule, unschedule
- Scheduled execution of tasks by the daemon (hourly/daily/weekly/monthly/`every Xm|Xh|Xd`)
- Task run history with status and output previews (`regin runs`)
- Long-term **memory** the agent loads as context every turn (fact / preference / pattern / project / skill / person)
- Configuration stored entirely in SQLite — **no config files**
- Per-user systemd integration: `daemon.enabled=true` installs a lingering user service
- Two skill layers: system (`/usr/share/regin/skills/`) and user (`~/.config/regin/skills/`); user overrides system by name
- Per-repo additions (context, memories, special skills): stored in regin's XDG store keyed by the repo's filesystem path — **not** committed into the repo (FEAT-008). The only in-repo footprint is dvalin's `.repo/dvalin/` + `.repo/project/`.

### Out of scope
- A `ROADMAP.md` file — **the roadmap is the collective `MILESTONE-*.md` files** under `.repo/project/issues/` (settled; do not create a ROADMAP.md)
- Replacing systemd / cron as a general scheduler (the scheduler exists only to drive tasks)
- Running agents remotely or multi-host fleet orchestration
- Bundling or hosting an LLM — regin is a client of the NanoGPT API
- Windows / macOS service integration (Linux + systemd is the target)

## 3. Language and build system

- **Language**: Rust (edition 2024)
- **Build**: `cargo build` / `cargo test` / `cargo build --release`
- **Workspace**: three crates — `regin-core` (lib), `regind` (daemon bin), `regin-cli` (`regin` bin)
- **Version**: `Cargo.toml` `[workspace.package] version` is the single source of truth

Key dependencies:
- `clap` (CLI parsing, derive)
- `tokio` + `tokio-stream` (async runtime, streaming)
- `rusqlite` (bundled SQLite — the durable store)
- `reqwest` (LLM + web-search HTTP, streaming)
- `serde` / `serde_json` / `toml` (serialization)
- `crossterm` + `ratatui` (terminal UI / streaming chat rendering)
- `anyhow` (error handling)
- `chrono`, `uuid`, `dirs`, `futures-util`, `tracing` + `tracing-subscriber`

## 4. Runtime architecture

```
regin (CLI) ──unix socket──▶ regind (daemon) ──HTTPS──▶ NanoGPT API (LLM)
     │                            │
     └──────── auto-start ────────┘
                                  ▼
                          SQLite (settings, runs, memories, conversations)
```

- **Socket**: `$XDG_RUNTIME_DIR/regin/regind.sock` (falls back to the data dir).
- **Database**: `<XDG_DATA_DIR>/regin/regin.db` — settings, task-run history, memories, chat history.
- **Skills**: system `/usr/share/regin/skills/`, user `~/.config/regin/skills/`.
- **Protocol**: typed `Request`/`Response` (`regin-core/src/protocol.rs`) framed over the Unix socket.
- The CLI carries **no LLM logic**; everything LLM- or DB-touching lives in the daemon.

## 5. Configuration (no config files)

All settings live in SQLite, seeded from `config::SETTINGS`:

| Key | Default | Meaning |
|---|---|---|
| `nanogpt.api_key` | _(empty — required)_ | NanoGPT API key |
| `nanogpt.model` | `claude-sonnet-4-20250514` | LLM model |
| `nanogpt.base_url` | `https://nano-gpt.com/api/v1` | API endpoint |
| `daemon.enabled` | `false` | Install + enable a lingering user systemd service for `regind` |

Managed via `regin config list|get|set`. Setting `daemon.enabled=true` writes a
user unit to `~/.config/systemd/user/` and enables lingering so the daemon
survives logout and starts at boot.

## 6. Consumers

- End-users running the `regin` CLI directly (operators, sysadmins).
- The `regind` daemon (scheduled, unattended task runs via the scheduler).
- Packaging targets that ship system skills under `/usr/share/regin/skills/`.

## 7. Build, install, package

```sh
cargo build --release        # builds regind and regin into target/release/
cargo test                   # unit tests
cargo build                  # debug build
```

Packaging: a Debian package layout lives under `pkg/` (`regin_<ver>_amd64/`
with `DEBIAN/control`, `postinst`, `prerm`, the binaries, system skills under
`usr/share/regin/skills/`, README, and man pages). Man pages: `man/regin.1`,
`man/regind.1`. A reference systemd unit is `regind.service`.

### Supported platforms

This is the governing supported-platform list for RULE-010 "Platform packages".

| Platform | Arch | Package format | Service manager |
|---|---|---|---|
| Linux (Debian/Ubuntu) | x86_64 | `.deb` | systemd (user service) |

Linux + systemd is the only first-class target. The daemon is a per-user
service (lingering), not a system daemon. Other platforms are out of scope
until a DISC ticket establishes demand.

## 8. Versioning and release phases

`Cargo.toml` is the single source of truth for the version.
Semver: **patch** for bug fixes, **minor** for new subcommands or flags,
**major** for breaking CLI changes.

regin uses the two-tier release model from the methodology:

### Tier 1 — Feature milestones (`0.x.0`)

```
features complete → alpha → beta → stable
```

The `Cargo.toml` version stays at the plain `0.x.0` number through all three
phases — no `-alpha.N` / `-beta.N` suffixes. The phase is tracked only in the
milestone file's `phase:` field.

| Phase | Cargo.toml version | Gate |
|---|---|---|
| Alpha | `0.x.0` | Features complete, unit tests green, clippy clean |
| Beta | `0.x.0` | No new features; integration tested; alpha bugs resolved |
| Stable | `0.x.0` | Beta signed off; release promoted |

### Tier 2 — Major release (`1.0.0`, ...)

A major release is the automatic outcome of all prerequisite feature
milestones reaching `phase: stable`. Milestone files carry a `phase:` field
(`alpha` | `beta` | `stable`).

## 9. Logging

Structured logging via `tracing` to **stderr**; stdout is reserved for
structured/user-facing data. The daemon defaults to `info`; the CLI defaults to
`warn`. Both honour `RUST_LOG` (`EnvFilter`). Keep secrets (API keys) out of
log output.

## 10. Testing

- **Unit tests** (`cargo test`, always) live in the same file as the code they
  test (Rust convention).
- Every bug fix must add a regression test (TDD: red → green).
- New features write the failing test first, commit it, then implement.

Highest-leverage units to cover: `config` (settings defaults + path
resolution), `skills` (system/user resolution + override), `db` (settings,
runs, memory round-trips), `tools` (each tool's success/error paths), and the
`protocol` request/response encoding.
