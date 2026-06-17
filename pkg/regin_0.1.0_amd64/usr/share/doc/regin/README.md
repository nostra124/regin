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
# Build
cargo build --release

# Set your NanoGPT API key
vim ~/.config/regin/config.toml
# nanogpt_api_key = "your-key-here"

# List skills
./target/release/regin skill list

# Run a skill
./target/release/regin skill run disk-usage

# Interactive chat
./target/release/regin chat

# Show config
./target/release/regin config

# Show task run history
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

Install as a systemd service:

```bash
sudo cp regind.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now regind
```

Manage:
```bash
systemctl status regind
journalctl -u regind -f
```

Flags:
- `--config <PATH>` — override config file location
- `--once` — run all skills once and exit

## Configuration

`~/.config/regin/config.toml`:

```toml
nanogpt_api_key = "your-nanogpt-api-key"
nanogpt_model = "claude-sonnet-4-20250514"
nanogpt_base_url = "https://nano-gpt.com/api/v1"
db_path = "~/.config/regin/regin.db"
skills_dir = "~/.config/regin/skills"
schedule_interval_secs = 3600
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `regin chat` | Interactive streaming chat with LLM |
| `regin skill list` | List available skills |
| `regin skill run <name>` | Execute a skill |
| `regin skill show <name>` | View skill prompt + files |
| `regin runs [--skill X]` | Show task run history |
| `regin config` | Display current config |

## License

MIT
