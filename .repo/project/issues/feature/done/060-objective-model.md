---
id: FEAT-060
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-033
---

# FEAT-060 — Objective model (maintain a state over time)

## Description
**As** regin
**I want** standing objectives that range over the KPI store and time windows
**So that** "maintain uptime > 99%/yr" is a first-class, monitored constraint.

## Implementation
- Generalize the DISC-008 to-be-state (FEAT-033) so an assertion may target a
  **KPI aggregate over a time window** (e.g. `kpi:uptime over 1y >= 0.99`), not only
  an instantaneous signal — evaluated against the KPI store (FEAT-050).
- An objective carries: the assertion(s), a **priority**, a **source** (dvalin-LLM /
  human / regin), and a **RAG health** (FEAT-064 computes it).
- An objective breach is a **deviation** that flows through the existing
  observed-vs-target / remediation loop (FEAT-034/037) — no parallel evaluator.
- Objectives are **hard constraints** in the intent optimization (goals are pursued
  without breaching them).

## Acceptance Criteria
1. An objective with a windowed-KPI assertion loads, evaluates against the KPI store,
   and flags a breach as a deviation.
2. Priority + source are persisted; RAG is queryable.
3. A breaching objective raises a deviation through the existing loop (not a new
   path); unit-tested with a seeded KPI history.
