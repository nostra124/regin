---
id: FEAT-035
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-011
depends_on: FEAT-002
---

# FEAT-035 — ITIL schema extensions

## Description
**As** regin
**I want** the incident/change/problem schema extended for the operator loop
**So that** blocking, problem-resolving changes, approvals, and hypotheses are
representable.

## Implementation
Idempotent additive migration on `regin-core` `types.rs` + `db.rs`:
- **Incident:** add a first-class `blocked` lifecycle status + a `workaround` TEXT
  field (parked on a workaround, awaiting a problem fix).
- **Change:** add `problem_id` (a change can resolve a *problem*, not just an
  incident); add a `pending_approval` state between `planned` and `applied`, with
  `approved_by` + `approved_at`.
- **Problem:** add a `problem_hypotheses` table (`id`, `problem_id`, `text`, `status`
  ∈ `created|validating|confirmed|rejected`, timestamps); optional temporary monitor
  attachment for long-run validation.
- **Cleanup:** drop the redundant `incidents.problem_id` column; keep the
  `problem_incidents` join (many incidents → one problem).
- CLI verbs to set/show the new fields (e.g. `incident block`, `change approve`,
  `problem hypothesis …`).

## Acceptance Criteria
1. Migration adds the new states/fields/table idempotently on a fresh and an existing
   DB.
2. An incident can enter `blocked` with a `workaround`; a change can link a
   `problem_id` and pass through `pending_approval` with approver/time recorded.
3. Problem hypotheses round-trip with status transitions.
4. `incidents.problem_id` is removed; the join table remains the linkage; unit-tested.
