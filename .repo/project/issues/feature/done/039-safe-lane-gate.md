---
id: FEAT-039
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-009
depends_on: FEAT-037
---

# FEAT-039 — Safe-lane gate (backout + dry-run + blast-radius)

## Description
**As** regin
**I want** a fix to qualify for auto-apply only if it has a concrete backout and
bounded blast radius
**So that** autonomous changes are always reversible (ITIL backout-plan discipline).

## Implementation
- Before auto-applying any change, **capture a concrete backout/undo first**:
  snapshot, backup, or proof the op is inherently reversible. No rollback plan ⇒ the
  change can never be auto-applied (it drops to `pending_approval`).
- **Dry-run runner** for ops that support one; verify expected effect before applying.
- **Blast-radius bound**: refuse the safe lane if the op's scope exceeds a configured
  bound (falls to approval).
- The captured backout is stored on the ITIL change so it can be executed to roll back.
- The pre-blessed allowlist (FEAT-037) is just a fast-path of ops already known
  reversible.

## Acceptance Criteria
1. A change with no captured backout cannot auto-apply (routes to approval).
2. A dry-run-capable op is dry-run before apply; a failed dry-run blocks auto-apply.
3. An op exceeding the blast-radius bound is refused the safe lane.
4. The backout plan is persisted on the change and is executable; unit-tested.
