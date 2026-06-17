---
id: FEAT-001
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 45-90min
phase: open
status: open
spawned_from: DISC-001
---

# Operations methodology doc set (ITIL discipline)

## Description
**As an** operator (human or agent) working in regin
**I want** a documented ITIL-flavoured operations methodology, the ops analogue
of dvalin's V-Model skills
**So that** incidents, changes, and problems are handled with consistent
discipline instead of ad-hoc.

dvalin's `.repo/project/skills/` describes a *development* process. regin needs
the parallel *operations* process. This ticket delivers the docs only — the
runtime data model and verbs are FEAT-002/003.

## Implementation
- Add `.repo/project/skills/operations-itil/` (or extend `operations/`) with:
  - `incident.md` — lifecycle (open → investigating → resolved → closed),
    severity/priority scale, what auto-derived incidents look like.
  - `change.md` — when to record a change, before/after documentation, link to
    the incident it remediates.
  - `problem.md` — recurrence threshold, known-error record, linking incidents.
  - `monitoring-triage.md` — how task-run results are evaluated into incidents.
- Add an ops-flavoured `profile`/overview note so the methodology states regin's
  operations remit (vs dvalin's development remit).
- Keep docs thin and pointed at the real verbs (`regin incident …` etc.) to
  avoid drift; this is discipline, not duplicated reference.
- Update `AGENTS.md` reading list only if a new top-level doc is added.

## Acceptance Criteria
1. An operator can read the ITIL doc set and know the incident/change/problem
   lifecycles and the severity scale without reading code.
2. The monitoring-triage doc describes exactly how a failing run becomes an
   incident and how recurrence becomes a problem (matching FEAT-004's behaviour).
3. Docs cross-link to the relevant verbs and to DISC-001.
4. No runtime/code changes in this ticket (docs only).
