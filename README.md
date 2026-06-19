# Regin

*Named after the dwarf smith from the Sigurd saga.*

An AI-powered monitoring and audit agent, written in Rust. Regin runs scheduled
"skills" (markdown-defined monitoring tasks) through an LLM and provides an
interactive chat interface.

## Architecture

```
┌──────────┐     ┌────────────┐     ┌───────────────┐
│  regin   │     │   regind   │     │   NanoGPT API │
│  (CLI)   │────▶│  (daemon)  │────▶│  (LLM)        │
└────┬─────┘     └─────┬──────┘     └───────────────┘
     │                 │
     └────────┬────────┘
              ▼
       ┌─────────────┐
       │   SQLite DB  │
       └─────────────┘
```

- **regin-core** — shared library: config, database, LLM client, skills engine
- **regind** — systemd daemon that runs skills on a schedule
- **regin** — CLI with interactive chat + skill management

## Quick Start

```bash
# Build (or install a native package — see Daemon below)
cargo build --release

# Set your NanoGPT API key (settings live in SQLite, not a config file)
./target/release/regin config set nanogpt.api_key "your-key-here"

# List operational tasks (skills) and run one
./target/release/regin task list
./target/release/regin task exec disk-usage

# Interactive chat
./target/release/regin chat

# Show settings and recent task-run history
./target/release/regin config list
./target/release/regin runs
```

## Skills

Skills live in `~/.config/regin/skills/`. Each skill is a directory containing:

- `skill.md` — the main prompt/instructions (first line = description)
- Optional supporting files (referenced as context)

Example structure:
```
skills/
├── disk-usage/
│   └── skill.md
├── security-audit/
│   ├── skill.md
│   └── checklist.md
└── uptime-report/
    └── skill.md
```

## Daemon (regind)

`regind` runs scheduled tasks and the operator loop in the background. It takes
**no command-line flags** — all settings live in SQLite (see Configuration).

Enable it as a per-user systemd service (regin auto-registers the unit):
```bash
regin config set daemon.enabled true
```

Or run it directly in the foreground:
```bash
regind
```

Inspect:
```bash
systemctl --user status regind
journalctl --user -u regind -f
regin ping            # check the daemon is up
```

## Configuration

regin is **config-file-free** — every setting lives in the SQLite DB and is
managed with `regin config`:

```bash
regin config list
regin config get nanogpt.model
regin config set nanogpt.api_key "your-nanogpt-api-key"
```

Common keys: `nanogpt.api_key`, `nanogpt.model`, `nanogpt.base_url`,
`daemon.enabled`, `monitor.recurrence_threshold`, `kpi.reliability_floor`.

## CLI Commands

| Command | Description |
|---------|-------------|
| `regin chat` | Interactive agent chat (tools: bash, files, web) |
| `regin task list \| show <n> \| exec <n> [--schedule X]` | Manage & run operational tasks (skills) |
| `regin runs [--skill X]` | Task-run history |
| `regin config list \| get \| set` | Settings (SQLite-backed) |
| `regin memory …` | Long-term memory |
| `regin incident \| change \| problem …` | ITIL records |
| `regin desired list \| show \| check` | Desired (to-be) state |
| `regin metrics` | CSI KPIs + cost-vs-reliability objective |
| `regin filters list \| test` | Notice filters |
| `regin mode` | Effective operating mode (org vs standalone) |
| `regin posture` | Adaptive autonomy posture |
| `regin greeting` | Login greeting: health + parked items |
| `regin push …` | Critical active-push channel (opt-in) |
| `regin checks` | Active promoted deterministic checks |
| `regin audit` | Run the CSI self-audit now |
| `regin context …` | Per-repo context store (keyed by repo path) |
| `regin skill …` | Skill-*package* manager (`regin-*-skills`) |
| `regin ping` | Check the daemon is running |
| `regin persona \| bus \| meeting \| plan \| foreman \| deputy` | Org / Mode-A coordination |

## License

MIT
