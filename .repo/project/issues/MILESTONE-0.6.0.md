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
| FEAT-021 | Portable `identity.db` + full schema | done |
| FEAT-022 | Migrate `episodes`/`memories` out of `regin.db` | done |
| FEAT-023 | Session archival + transcript capture | done |
| FEAT-024 | Consolidation pipeline (Curator) | done |
| FEAT-025 | Activation-ranked retrieval | done |
| FEAT-026 | Vector/embedding recall | done |
| FEAT-027 | Portability verbs (`memory export/import`) | done |

### Decision plane (DISC-018)

| ID | Title | Status |
|----|-------|--------|
| FEAT-028 | Dual-mode agent loop (act vs deliberate) | done |
| FEAT-029 | The Soul gate (values-grounded vote + veto) | done |
| FEAT-030 | Soul configurator + value catalog | done |
| FEAT-031 | Principle derivation & ratification | done |
| FEAT-032 | Deliberation capture | done |

### Test-coverage to 100% (DISC-020 — folded in; completes the 0.5.0 exit criterion)

| ID | Title | Status |
|----|-------|--------|
| FEAT-070 | CLI transport seam + render/logic split | done |
| FEAT-071 | Injectable LLM client (`LlmClient` trait) | done |
| FEAT-072 | llm.rs pure extraction + mock-HTTP test | open |
| FEAT-073 | Daemon loop extraction + full dispatch coverage | open |
| FEAT-074 | Integration tests over the real binaries | open |
| FEAT-075 | Easy-win unit tests + coverage gate ramp to 100% | open |

The testability seams (FEAT-070/071) also make the identity-plane features above
easier to test, so land them early. Target: absolute 100% line coverage, no
exclusions — the binary glue is covered by FEAT-074 spawning the real instrumented
binaries (cargo-llvm-cov captures child-process coverage).

## Notes

- The decision-plane FEATs (028–032) depend on the memory-plane store (FEAT-021)
  and consolidation (FEAT-024); the memory plane lands first within this milestone.
- All twelve feature files (021–032) are minted under `feature/open/`.
- DISC-018 reuses DISC-009's blast-radius/reversibility judgement as the trigger for
  deliberate mode; FEAT-031 reuses the escalation bridge (FEAT-015, shipped in
  0.4.0) for human ratification.

## Suggested delivery order

1. **FEAT-021** — `identity.db` + schema (foundation; everything else builds on it).
2. **FEAT-022** + **FEAT-023** — migrate legacy memory in; start capturing sessions/
   transcripts (populate the store).
3. **FEAT-024** — consolidation / Curator (the learning loop).
4. **FEAT-025** → **FEAT-026** — activation-ranked retrieval, then additive vector
   recall.
5. **FEAT-027** — portability verbs (independent; any time after 021).
6. Decision plane: **FEAT-028** (loop) → **FEAT-030** (value catalog + configurator)
   → **FEAT-029** (Soul gate) → **FEAT-032** (deliberation capture) → **FEAT-031**
   (principle derivation & ratification; needs 024 + 032).

## Exit criteria

- `identity.db` is the **single source of truth** for the memory plane; `regin.db`
  no longer holds `episodes`/`memories` (FEAT-022 complete).
- Memory **consolidates** (interference-resolved), **recalls** (FTS + activation,
  vector additive), and **decays**; the identity is **portable** (`memory
  export/import`, per-host scoping).
- Consequential/irreversible actions route through **deliberate mode**; the **Soul
  gate** can veto and escalate; values are **configurable** (catalog + Persona
  overlay) and principles are **human-ratified**; deliberations are **captured** and
  feed calibration.
- 100% test coverage (per the milestone delivery-prerequisite pattern); no open
  design questions remain in any 021–032 FEAT (RULE-005).
