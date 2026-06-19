---
id: DISC-008
type: discovery
priority: high
status: open
complexity: M
spawned_features: ~
---

# DISC-008 — Machine to-be-state model (the operator's reference state)

## Operating-plane context

regin runs in two planes. This DISC (and DISC-009..011) concern the **operator
plane** only: regin as the autonomous operator of the *machine/container* it runs
on, governed by ITIL (to-be state · incident · problem · change). The
**foreman/repo-worker plane** — working inside a repo under that repo's own
methodology — is a separate concern discussed elsewhere. ITIL never applies inside
a repo; repo methodology never governs the machine.

## Describe

The operator's entire job is to keep the machine at its **to-be (target) state**.
Today regin has no representation of that target: monitoring skills are
report-only prompts that compare observations against nothing, and an "incident"
is really just "a scheduled run errored" (`monitoring-triage.md`), not "the system
deviated from its declared target". Without a reference state there is no
principled definition of *deviation*, and no basis for deciding when the machine
is back to good.

The to-be state has **three layers** (converged with user — see Decision):
- **Explicit markdown** — operator-/admin-authored desired-state narrative ("what
  good looks like": services that must be up, disk/inode/cpu expectations, expected
  packages, open ports, cert validity, time sync, backup freshness). The intent.
- **Structured** — machine-checkable assertions/thresholds (hard pass/fail) that a
  deterministic check can evaluate without an LLM.
- **Implicit** — thresholds and expectations encoded in the monitor skills
  themselves (as today).

## Variants considered

| Variant | Summary | Key trade-off |
|---|---|---|
| A | Explicit markdown desired-state docs + implicit thresholds in monitor skills | Human-readable, LLM-native, easy to edit; less machine-checkable |
| B | Pure implicit — thresholds only live in skills/config, no explicit doc | Simplest; but no single source of "target", nothing to show the human |
| C | Structured schema (TOML/YAML) desired-state with typed checks | Machine-verifiable; rigid, more upfront modelling, less LLM-native |

## Decision matrix

| Criterion | Weight | A | B | C |
|---|---|---|---|---|
| LLM-native / editable by operator | high | ✓ | ~ | ✗ |
| Single source of "target" to show human at login | high | ✓ | ✗ | ✓ |
| Implementation cost now | med | ✓ | ✓ | ~ |
| Machine-checkable deviation | med | ~ | ~ | ✓ |

**Leaning:** Variant **A** — explicit markdown desired-state, complemented by the
implicit thresholds already in monitor skills. Storage candidate: regin's XDG
store keyed by host, plus an admin-editable `/etc/regin/desired/*.md` overlay.

## Decision (resolved with user — guided Q&A 2026-06-19)

**Q1 — Representation: all three layers.** The to-be state is expressed as
**explicit markdown** (intent/narrative), **structured assertions**
(machine-checkable hard pass/fail), and **implicit thresholds** in monitor skills.
The three are independent statements of the same target and must agree. This yields
a clean two-way split:
- **Deviation from target** (reality ≠ declared state) → **incident** (a runtime
  issue, by-definition solvable).
- **Conflict *within* the target** (markdown says X, structured says Y) →
  **problem** (the *definition* is ambiguous/contradictory; it cannot be
  auto-fixed and needs human resolution).

**Q2 — Storage: files, like skills.** Desired-state lives as **files** an admin
edits with `$EDITOR`/git, read by regin — *not* in the SQLite store. This is a
deliberate, scoped exception to the standing "all state in SQLite, no config files"
rule: desired-state is **authored content** (like skills), not runtime state, so it
follows the skills precedent. It layers like skills — a system location plus a
user/admin override. Exact path is pinned at design time: because `regind` is a
**per-user** service, the natural home is `~/.config/regin/desired/` (with a
possible `/etc/regin/desired/` system overlay), mirroring
`~/.config/regin/skills/` over `/usr/share/regin/skills/`.

**Q3 — Granularity: per-domain files.** Split per domain (`disk.md`, `services.md`,
`security.md`, `network.md`, `certs.md`, `backups.md`, …); regin reads every file
in the directory. Domains map **1:1 to the operator skill catalog** (DISC-012): a
domain's desired-state doc pairs with that domain's monitor skill(s).

**Q4 — What counts as a deviation: LLM judgment, not raw events.** *Not every
monitoring event is an incident* — exercising that judgment against the intended
state is the whole point of using an LLM. An incident is raised when the agent
**judges** an observation to be materially off the to-be state, evaluated against
all three layers (structured assertions give deterministic pass/fail; markdown
intent is LLM-judged; implicit thresholds as today). The *economics* of producing
that judgment cheaply — periodic LLM review, promotion of crystal-clear cases to
cheap deterministic checks, per-log notice filters, and measuring cost vs.
reliability — is a discovery of its own → **DISC-015** (adaptive monitoring
economics; ITIL CSI).

## Spawned features (to derive on close)

- Desired-state file format + loader: per-domain markdown + a structured
  assertions block, layered user-over-system (like skills), read by `regind`.
- Redefine monitoring evaluation as *observed-vs-target* (supersedes the
  "run-errored = incident" framing in `monitoring-triage.md` / FEAT-004), with the
  three-layer target as the reference.
- Conflict detector: disagreement between the markdown and structured layers opens
  a **problem** (not an incident).
- `regin chat` login greeting shows the current to-be state per domain and where
  reality deviates (feeds DISC-010).
