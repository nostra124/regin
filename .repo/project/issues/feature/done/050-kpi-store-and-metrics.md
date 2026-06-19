---
id: FEAT-050
type: feature
priority: high
complexity: L
estimate_tokens: 60k-110k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-015
depends_on: FEAT-002
---

# FEAT-050 — KPI store + `regin metrics`

## Description
**As** regin (and the operator)
**I want** the operator KPIs tracked and surfaced
**So that** the CSI loop can prove cost is falling while reliability holds — and steer
promotion/autonomy on evidence.

## Implementation
- Full KPI schema (all four groups) in SQLite alongside ITIL records:
  reliability-as-constraint, time-in-deviation, **automation ratio**, notice-filter
  savings, cost-avoided, MTTD/MTTR, recurrence, **promotion-error rate**, **autonomy
  ratio**.
- **Constrained-objective** evaluation: minimise cost subject to reliability ≥ floor;
  north-star = cost ↓ while time-in-deviation ↓; trend tracking over time.
- Surface: a **CSI summary in the login greeting** (FEAT-043) + a **`regin metrics`**
  command.
- Promotion-error / autonomy KPIs report once their features (FEAT-051/040) land.

## Acceptance Criteria
1. KPIs persist beside ITIL records and expose trends; the constrained objective is
   computed (cost s.t. reliability ≥ floor).
2. `regin metrics` renders the KPI summary; the CSI summary appears in the greeting.
3. KPIs are consumable by promotion (FEAT-051) and adaptive posture (FEAT-040);
   unit-tested.
