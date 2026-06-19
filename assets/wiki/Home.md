# regin

**regin** is an LLM-backed agent that operates a Linux/UNIX machine. It runs
unattended, keeps the system at its declared *to-be state*, and applies ITIL
discipline (incidents → changes → problems) to everything it does.

## Two operating modes

- **(A) dvalin foreman** — regin acts as a role/persona on a multi-agent bus,
  taking direction from a supervisor and escalating decisions and code/config
  work to the development plane.
- **(B) standalone operator** — regin runs 24/7 as the independent operator of one
  machine: scheduled monitors, observed-vs-target evaluation, autonomous
  remediation within guardrails, and a pull-at-login greeting when no supervisor
  is reachable.

## Install

One command (auto-detects deb/rpm/apk):

```sh
curl -fsSL https://raw.githubusercontent.com/nostra124/regin/main/assets/install.sh | sh
```

Or grab the `.deb` / `.rpm` / `.apk` for your distro from the
[latest release](https://github.com/nostra124/regin/releases/latest).

## Quickstart

```sh
regin config set nanogpt.api_key <your-key>   # configure the LLM
regin chat                                     # talk to the operator (opens with a health greeting)
regin task list                                # see available skills
regin desired list                             # the machine's to-be state, per domain
regin metrics                                  # CSI KPIs + the cost-vs-reliability objective
```

## How the operator loop works

1. **Monitor** — operator skills gather signals on their own cadence (with jitter
   to smooth load).
2. **Evaluate** — signals are judged *observed vs target* against each domain's
   to-be-state assertions; a genuine deviation raises an incident.
3. **Remediate** — a candidate fix is routed to one of three lanes: auto-apply
   (safe + reversible, within the capability ceiling and earned autonomy posture),
   pending-approval (risky — staged for a human/supervisor), or escalate (out of
   regin's control → a problem). Global red-lines can never be crossed.
4. **Learn** — stable LLM verdicts are promoted into cheap deterministic checks;
   notice filters drop known noise; a periodic self-audit keeps it all honest;
   KPIs prove cost is falling while reliability holds.

## Command overview

| Command | What it does |
|---|---|
| `regin chat` | Converse with the operator (opens with the login greeting) |
| `regin task` | List / show / run / schedule skills |
| `regin runs` | Recent scheduled-run history |
| `regin config` | Get / set / list settings |
| `regin memory` | Manage self-curated memory (Hermes) |
| `regin incident` / `change` / `problem` | ITIL records (incl. `incident block`, `change approve`, `problem hypothesis-*`) |
| `regin desired` | Inspect the to-be state per domain (`list` / `show` / `check`) |
| `regin metrics` | CSI KPIs + the constrained objective |
| `regin filters` | Notice filters that suppress known noise |
| `regin mode` | Effective mode: org (supervisor) vs standalone |
| `regin posture` | Adaptive autonomy posture + its evidence |
| `regin greeting` | Health + parked items needing a decision |
| `regin push` | Critical-only active push (opt-in) |
| `regin checks` | Promoted deterministic checks |
| `regin audit` | Run the CSI self-audit now |

Full flags are in the generated man pages (`man regin`) and `regin <command>
--help`.

## Learn more

- [Project profile](https://github.com/nostra124/regin) — design, modes, and the
  supported-platform list.
- Man pages ship with the package and are generated from the CLI, so they never
  drift from the actual commands.
