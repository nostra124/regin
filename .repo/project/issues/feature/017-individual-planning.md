---
id: FEAT-017
type: feature
status: open
milestone: 0.4.0
disc: DISC-006
---
# FEAT-017 — Individual planning cycle (aggregate When/Which → plan; emit upward)

Each regin plans its own work from its decentralized signals (DISC-006): per-repo
schedules (When) + per-repo required skills (Which) + ITIL backlog, on a cadence
(weekly/monthly/yearly). Emits upward signals: priority asks to the process owner,
capability gaps to CAO.

- planning::build_plan(cadence, schedules, skills, itil_counts) → Plan, pure.
- upward signals: structured messages (priority asks; capability gaps).
- CLI `regin plan [--cadence] [--emit]` reading the db, optionally emitting.

Acceptance: a plan aggregates schedules/skills/ITIL into a cadence plan; capability
gaps and priority asks build well-formed upward messages. Unit-tested.
