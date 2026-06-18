# Incident management

An **incident** is an unplanned interruption or degradation. Goal: restore
service quickly, then record enough to learn from it.

## Lifecycle

```
open ──▶ investigating ──▶ resolved ──▶ closed
```

| Status | Meaning |
|---|---|
| `open` | newly raised, not yet being worked |
| `investigating` | being actively diagnosed |
| `resolved` | service restored; a resolution note is recorded |
| `closed` | confirmed and filed |

## Severity scale

| Severity | Use for |
|---|---|
| `critical` | full outage / data at risk — drop everything |
| `high` | major function down or degraded for many |
| `medium` | contained impact, workaround exists (default) |
| `low` | minor / cosmetic / single-user |

## Verbs

```
regin incident open "<title>" --severity high --desc "<detail>"
regin incident list [--status open|investigating|resolved|closed]
regin incident show <id>
regin incident update <id> --status investigating
regin incident resolve <id> "<resolution>"
regin incident close <id>
```

## Source

`source = manual` (an operator opened it) or `source = monitor` (auto-derived
from a failed scheduled run — see `monitoring-triage.md`). Monitor incidents
carry the `skill_name` that produced them and may be linked to a `problem`.
