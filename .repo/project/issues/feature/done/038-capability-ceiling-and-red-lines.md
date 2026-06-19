---
id: FEAT-038
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-009
depends_on: FEAT-011
---

# FEAT-038 — Capability ceiling + global red-lines

## Description
**As** the system owner
**I want** an editable operator capability ceiling under a static global red-line set
**So that** regin's autonomy is bounded — and can't be talked into widening itself.

## Implementation
- **Operator-role capability ceiling** (editable policy, building on FEAT-011's
  per-persona tool ceiling): the operator role's day-to-day authorization floor.
- **Static global red-lines** (non-runtime-adjustable, compiled-in) that no role may
  ever cross:
  - protect the safety substrate — never delete backups/snapshots, tamper with/disable
    the audit log, or erase the KPI store;
  - don't sever governance — never break its own service, cut the escalation channel,
    or disable the human kill-switch;
  - no catastrophic host actions — `rm -rf /`, wipe the data dir, `dd`/repartition,
    rewrite `/etc/shadow` or add a root-equiv user, disable the firewall wholesale.
- Every action checked against **both** layers; denials carry a clear "which layer
  denied this" audit message.
- Rationale: the ceiling is editable + regin ingests logs (a prompt-injection
  surface), so the global layer is defense-in-depth (constitutional vs statutory).

## Acceptance Criteria
1. An action outside the operator ceiling is refused with a "ceiling" reason.
2. A red-line action is refused even if the ceiling would allow it, with a "red-line"
   reason; red-lines are not runtime-editable.
3. Allowed actions pass; audit messages name the deciding layer; unit-tested.
