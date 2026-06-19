---
id: DISC-011
type: discovery
priority: medium
status: open
complexity: M
spawned_features: ~
---

# DISC-011 — ITIL model extensions (blocking, problem→change, approval, hypotheses)

## Operating-plane context

Operator plane (see DISC-008). Concerns the data-model changes the operator loop
needs beyond what FEAT-002 shipped.

## Describe

The converged operator model needs these additions to the existing
incident/change/problem schema (`regin-core/src/types.rs`, `db.rs`):

1. **Incident blocked-by-problem + workaround.** An incident can be *blocked* by a
   problem and run on a workaround until the real fix lands. Today
   `incident.problem_id` exists but there is no `blocked` status and no
   `workaround` field. Add a `blocked` lifecycle state and a `workaround` note.
2. **Change resolves a problem.** The real fix for a chronic problem is a change
   that rides out of the *problem* (worked example: adjust log rotation). Today
   `change` links only to `incident_id`. Add **`change.problem_id`**.
3. **Change approval gate.** Per DISC-009, a change can be `pending_approval`
   between `planned` and `applied`. Add the state (and who/when approved).
4. **Problem investigation.** A problem needs hypotheses tracked over time
   (created → validating → confirmed/rejected) and may attach a temporary monitor
   for long-run validation. Today `Problem` carries only `root_cause`.
5. **Cleanup.** The schema carries *both* `incidents.problem_id` and a
   `problem_incidents` join table — redundant. Keep the join table (it models
   "many incidents → one problem", which is core), drop the column.

## Variants considered (selected points)

| Point | Options | Leaning |
|---|---|---|
| Incident "blocked" | new status vs. orthogonal `blocked_by` flag | new lifecycle status `blocked` (+ `workaround` text) |
| Problem hypotheses | structured rows (text+status) vs. freeform notes | lightweight structured rows; defer experiment framework |
| Approval record | flag vs. `pending_approval` status + approver/approved_at | status + approver/approved_at (audit trail) |

## Decision matrix

| Criterion | Weight | Lightweight (leaning) | Heavy (full experiment/CMDB) |
|---|---|---|---|
| Covers the model | high | ✓ | ✓ |
| Implementation cost | high | ✓ | ✗ |
| Migration risk | med | ✓ | ~ |

**Leaning:** lightweight structured additions; no full CMDB/experiment framework
yet.

## Open questions (resolving with user)

1. Is `blocked` a first-class incident status, or just derived from "linked to an
   open problem with no workaround"?
2. Hypothesis tracking depth — minimal (list + status) for now?
3. Recurrence threshold (default 3) — keep, or make it part of the to-be-state doc?

## Decision

_Pending — being resolved with the user (guided Q&A)._

## Spawned features

_Pending DISC close._
