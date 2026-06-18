---
id: FEAT-016
type: feature
status: done
milestone: 0.4.0
disc: DISC-004
---
# FEAT-016 — Meeting-chair: run agenda → minutes + action-items over the bus

A regin role chairs a standing meeting (DISC-004): runs the standard agenda,
collects participants' reports off the bus, applies discipline, and produces
minutes + decisions + action-items posted back to dvalin (which records them,
dvalin FEAT-133).

- chair::compile_minutes(agenda, reports) → Minutes (decisions + action-items),
  pulling regin's own ITIL/self-improvement counts into the agenda.
- builds the structured minutes message for dvalin `meeting run`.
- CLI `regin meeting chair <name>` (collect inbox reports → emit minutes).

Acceptance: an agenda + collected reports compile into minutes with decisions
and action-items; the minutes message is well-formed. Unit-tested.
