# v0.2.0 — Operations discipline: ITIL, self-improving memory, skill authoring

This milestone turns regin from a scheduled-skill runner into a disciplined
**operations agent**. It adopts a small ITIL process (incident / change /
problem) backed by both runtime SQLite records and an operations methodology
doc set (DISC-001); adds the "Hermes" self-improving, tiered episodic→semantic
memory loop so regin learns from its own activity (DISC-002); makes monitoring
results actionable by auto-deriving incidents and surfacing recurring problems;
and adds first-class skill authoring. It also closes the gap where entering chat
spawns a loose daemon instead of registering the persistent systemd service.

Sequencing: the data/foundation tickets (FEAT-002, FEAT-005) land before the
surface/loop tickets that depend on them (FEAT-003/004, FEAT-006). FEAT-001
(docs) can land any time and should track the verbs as they ship. BUG-001 and
FEAT-007 are independent quick(er) wins.

## Tickets

| ID | Title | Depends on | Status |
|----|-------|------------|--------|
| DISC-001 | Regin as an ITIL operations agent | — | **done** |
| DISC-002 | Hermes: self-improving, tiered memory | — | **done** |
| BUG-001 | Chat auto-registers the systemd user service | — | **done** |
| FEAT-002 | ITIL data model in SQLite | — | **done** |
| FEAT-003 | ITIL CLI verb families (incident/change/problem) | FEAT-002 | **done** |
| FEAT-004 | Monitoring evaluation → auto incidents; recurrence → problems | FEAT-002 | **done** |
| FEAT-005 | Episodic memory tier | — | **done** |
| FEAT-006 | Reflective distillation (episodic → semantic) | FEAT-005 | **done** |
| FEAT-007 | Skill (task) creation flow | — | **done** |
| FEAT-008 | Per-repo additions (context/memories) in XDG store, keyed by repo path | — | **done** |
| FEAT-009 | Per-repo skills layer in XDG store (split from FEAT-008) | FEAT-008 | **done** |
| FEAT-001 | Operations methodology doc set (ITIL discipline) | — | **done** |

## Out of scope (future milestone)

- **DISC-003 — regin as a dvalin dwarf**: integrating regin as an executor/agent
  in dvalin's workflow engine (ticket escalation, software-dev steps). Captured
  as a discovery item; no implementation in 0.2.0.
