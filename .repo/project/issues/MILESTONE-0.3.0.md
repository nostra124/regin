# v0.3.0 — Cave foreman & messaging-bus client

regin becomes a first-class citizen of the dvalin organization: it runs as the
**foreman of a cave** (`regin@cave`), speaks dvalin's **messaging bus**, supervises
local CLI workers (claude/opencode), and adopts a **role persona** with its
**skill packages** deployed by dvalin. This is regin's half of the integration MVP
(dvalin MILESTONE-1.3.0).

## Source discovery
- **DISC-004** — foreman mode + messaging-bus client + local worker supervision.
- **DISC-005** — role personas + capability(=tool) scoping (a regin *becomes* a role).
- **DISC-007** — standard skill catalog & role/area skill-packages.

## Scope (FEATs derived on DISC close)
- `regind` messaging-bus client (send/inbox/subscribe via execd; identity; two modes).
- Foreman mode: cave-task intake → decompose → supervise local CLI workers → structured handover up.
- Role-persona config + loader; per-role capability/tool enforcement (authorization ceiling).
- Skill-package structure (`regin-base-skills` + role/area packages) + build.

## Depends on
- dvalin **MILESTONE-1.1.0** (bus) + **1.2.0** (skill deployment). Cross-repo.

## Out of scope
- A `ROADMAP.md` (the roadmap is the collective `MILESTONE-*.md` files).
