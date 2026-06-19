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

The to-be state has two sources (per user):
- **Explicit** — operator-/admin-authored desired-state ("what good looks like":
  services that must be up, disk/inode/cpu thresholds, expected packages, open
  ports, cert validity, time sync, backup freshness).
- **Implicit** — grounded out of monitoring (thresholds and expectations encoded
  in the monitor skills themselves).

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

## Open questions (resolving with user)

1. Storage location: XDG-keyed only, or also an admin-editable `/etc/regin/desired/`?
2. One desired-state doc, or per-domain (disk, services, security, …)?
3. Is "deviation → incident" redefined as *observed vs target*, or kept as "run
   failed" with the target doc as context only?

## Decision

_Pending — being resolved with the user (guided Q&A)._

## Spawned features

_Pending DISC close._
