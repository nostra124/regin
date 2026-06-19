---
id: FEAT-033
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-008
depends_on: FEAT-004
---

# FEAT-033 — Desired-state (to-be) format + loader

## Description
**As** an operator
**I want** to declare the machine's to-be state in per-domain files
**So that** regin has an explicit reference to judge "observed vs target" against.

## Implementation
- Per-domain desired-state files (`disk.md`, `services.md`, …) mapping 1:1 to the
  operator skill catalog (DISC-012), each with a **markdown intent** section + a
  **structured assertions** block (machine-checkable).
- Stored as files (a scoped, deliberate exception to "all state in SQLite", following
  the skills precedent): `~/.config/regin/desired/` over a possible
  `/etc/regin/desired/`; **layered user-over-system** like skills; read by `regind`.
- **Conflict detector:** when the markdown intent and the structured assertions
  disagree, open a **problem** (the target is ambiguous — needs a human), *not* an
  incident.
- Loader validates/parses on startup + on change; bad files fail safe (logged, prior
  good state retained).
- Optional per-domain `recurrence_threshold` (consumed by FEAT-036).

## Acceptance Criteria
1. A per-domain desired-state file with both layers loads and is queryable by domain.
2. User files override system files by domain name (user-over-system).
3. A markdown/structured conflict opens a problem (not an incident); agreement does
   not.
4. Malformed files fail safe (logged; last good state kept); unit-tested.
