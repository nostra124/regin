# Change management

A **change** is a deliberate modification to a system. Document it so there is an
audit trail of *what was done* — especially the remediation of an incident.

## Lifecycle

```
planned ──▶ applied ──▶ closed
```

| Status | Meaning |
|---|---|
| `planned` | recorded, not yet applied |
| `applied` | executed (records the time) |
| `closed` | verified and filed |

## Discipline

- Record a change **before** applying it where possible; capture `before` and
  `after` state so the effect is auditable and reversible.
- Link the change to the **incident** it remediates (`--incident <id>`) so the
  operations record is traceable end to end.

## Verbs

```
regin change record "<title>" --desc "<detail>" \
    --incident <incident-id> --before "<state>" --after "<state>"
regin change list
regin change show <id>
regin change apply <id>
regin change close <id>
```
