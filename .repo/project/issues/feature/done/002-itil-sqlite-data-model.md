---
id: FEAT-002
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 60-120min
phase: done
status: done
spawned_from: DISC-001
---

# ITIL data model in SQLite (incident / change / problem)

## Description
**As** regin-core
**I want** durable SQLite tables and a typed data layer for incidents, changes,
and problems
**So that** both the CLI and the daemon (monitoring evaluation) can create and
mutate operational records.

This is the foundation FEAT-003 (verbs) and FEAT-004 (auto-creation) build on.

## Implementation
- New tables in `db::init_db` (idempotent `CREATE TABLE IF NOT EXISTS`):
  - `incidents` — id, title, description, severity, status
    (`open|investigating|resolved|closed`), source (`manual|monitor`),
    skill_name (nullable, when monitor-derived), problem_id (nullable),
    opened_at, updated_at, resolved_at, resolution.
  - `changes` — id, title, description, status (`planned|applied|closed`),
    incident_id (nullable, the incident it remediates), before, after,
    created_at, applied_at.
  - `problems` — id, title, description, status (`open|known_error|closed`),
    root_cause (nullable), created_at, updated_at, closed_at.
  - `problem_incidents` — link table (problem_id, incident_id) for many-to-one.
- Add typed structs to `regin-core/src/types.rs` (`Incident`, `Change`,
  `Problem`) with serde.
- Add `db` functions: create/list/get/update-status/close for each, plus
  `link_incident_to_problem`. Follow existing `memory_*` / `task_run` patterns.
- IDs: reuse the existing short-id scheme (uuid v4 truncated, as memories do).

## Acceptance Criteria
1. `init_db` creates all four tables; running twice is a no-op (idempotent).
2. Round-trip unit tests: create → get → update status → close for each of
   incident/change/problem, plus problem↔incident linking.
3. Structs serialize/deserialize via serde and are re-exported from `lib`.
4. No CLI surface yet (that is FEAT-003) — pure data layer + tests.
