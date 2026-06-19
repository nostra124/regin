---
milestone: 0.6.0
title: Identity plane — portable memory + decision modes (Persona / Mind / Soul)
status: planned
depends_on: 0.5.0
---

# Milestone 0.6.0 — Identity plane

The operator milestones (0.3.0–0.5.0) build the **machine apparatus** — how regin
runs a box. This milestone builds the **agent identity** — the part of regin that is
*itself* and travels between machines. Two discoveries define it:

- **DISC-017 (memory plane)** — *what regin knows*: a portable `identity.db`,
  consolidation, activation-ranked + vector recall, portability verbs.
- **DISC-018 (decision plane)** — *how regin decides*: the **Persona / Mind / Soul**
  model and two runtime modes (act vs deliberate), with a values-grounded Soul gate.

## Sharp vocabulary (used across these tickets)

| Term | One line |
|---|---|
| **Persona** | the *outward* identity — the role regin acts as (FEAT-011) |
| **Mind** | the *reasoning* — plans and decides |
| **Soul** | the *inner* identity — the values-grounded conscience that gates the plan |
| **Body** | *execution* — tool dispatch (already built) |

Modes: **act** = `Mind → Body` (fast default); **deliberate** = `Mind ⇄ Soul → Body`
(read-only plan, Soul votes, approved plan executed).

## Issues

### Memory plane (DISC-017)

| ID | Title | Status |
|----|-------|--------|
| FEAT-021 | Portable `identity.db` + full schema | open |
| FEAT-022 | Migrate `episodes`/`memories` out of `regin.db` | open |
| FEAT-023 | Session archival + transcript capture | open |
| FEAT-024 | Consolidation pipeline (Curator) | open |
| FEAT-025 | Activation-ranked retrieval | open |
| FEAT-026 | Vector/embedding recall | open |
| FEAT-027 | Portability verbs (`memory export/import`) | open |

### Decision plane (DISC-018)

| ID | Title | Status |
|----|-------|--------|
| FEAT-028 | Dual-mode agent loop (act vs deliberate) | open |
| FEAT-029 | The Soul gate (values-grounded vote + veto) | open |
| FEAT-030 | Soul configurator + value catalog | open |
| FEAT-031 | Principle derivation & ratification | open |
| FEAT-032 | Deliberation capture | open |

## Notes

- The decision-plane FEATs (028–032) depend on the memory-plane store (FEAT-021)
  and consolidation (FEAT-024); the memory plane should land first within this
  milestone.
- All twelve feature files (021–032) are now minted under `feature/open/`.
