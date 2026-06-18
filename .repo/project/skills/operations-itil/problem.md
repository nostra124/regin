# Problem management

A **problem** is the underlying cause behind one or more incidents — especially
**recurring** ones. Incidents restore service; problems remove the cause.

## Lifecycle

```
open ──▶ known_error ──▶ closed
```

| Status | Meaning |
|---|---|
| `open` | a cause is suspected; incidents are being linked |
| `known_error` | root cause identified and recorded (a workaround may exist) |
| `closed` | root cause fixed (usually via a `change`) |

## How problems arise

- **Manually**, when an operator spots a pattern.
- **Automatically**, when the monitor evaluator sees the recurrence threshold of
  same-shape incidents reached (see `monitoring-triage.md`). The contributing
  incidents are linked to the problem.

## Verbs

```
regin problem open "<title>" --desc "<detail>"
regin problem list [--status open|known_error|closed]
regin problem show <id>
regin problem link <problem-id> <incident-id>
regin problem known-error <id> "<root cause>"
regin problem close <id>
```
