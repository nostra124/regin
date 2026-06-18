---
id: DISC-001
type: discovery
priority: high
status: done
complexity: L
spawned_features: [FEAT-001, FEAT-002, FEAT-003, FEAT-004]
---

# DISC-001 — Regin as an ITIL operations agent

## Describe

dvalin is a **project/development** supervisor with a V-Model methodology.
regin is its operations counterpart: an AI agent for **maintenance and
operations** of running systems. Today regin can chat, run markdown skills on
a schedule, store run history, and hold long-term memories — but it has no
*operational discipline*: monitoring runs produce output that nobody triages,
and there is no record of what broke, what was changed, or what root causes
recur.

We want regin to adopt a **small ITIL process**: Incident, Change, and Problem
management, with the same "discipline-as-code" approach dvalin uses for
development. Concretely:

- **Incident** — an unplanned interruption / degradation. Opened (by a human or
  auto-derived from a monitoring result), worked, and resolved.
- **Change** — a deliberate modification to a system. Documented before/after so
  there is an audit trail of what regin (or the operator) did.
- **Problem** — the underlying cause behind one or more incidents, especially
  **recurring** ones. Opened when a pattern emerges; closed with a known-error /
  root cause.

Monitoring results (task runs) must be **evaluated** so that a failing or
anomalous run can spawn an incident, and repeated incidents of the same shape
can later be promoted to a problem.

## Variants considered

| Variant | Summary | Key trade-off |
|---|---|---|
| A | **Runtime records only** — SQLite tables + verbs, no methodology docs | Fast to ship; but no shared discipline, operators improvise process |
| B | **Methodology docs only** — markdown ITIL process under `.repo/`, no data model | Discipline without tooling; nothing for monitoring to auto-create |
| C | **Both** — SQLite records + verbs AND an ops-methodology doc set mirroring dvalin | More work up front; but discipline *and* tooling, and monitoring can act |

## Decision matrix

| Criterion | Weight | A | B | C |
|---|---|---|---|---|
| Monitoring can auto-create incidents | high | ✓ | ✗ | ✓ |
| Shared, reviewable operational discipline | high | ✗ | ✓ | ✓ |
| Parity with dvalin's "discipline-as-code" | med | ✗ | ~ | ✓ |
| Time-to-first-value | med | ✓ | ✓ | ~ |

## Arguments

### Pro (Variant C — both)

- Monitoring → incident automation **requires** a runtime data model; a doc-only
  approach cannot record state the daemon mutates.
- The user explicitly asked for "discipline similar like dvalin … regin is for
  maintenance/operations" — that is a methodology, not just a table.
- The two reinforce each other: the methodology defines *lifecycle and gates*;
  the SQLite records are the *instances* that flow through that lifecycle, the
  same way dvalin's `issues/` tickets flow through its V-Model.

### Con / risks

- Larger surface; must be sequenced (data model → verbs → evaluation → docs).
- Risk of the methodology drifting from the implemented state — mitigate by
  keeping the ops-methodology doc set thin and pointing at the real verbs.

## Decision

**Chosen:** Variant C — **both** SQLite runtime records and an operations
methodology doc set.

**Why:** Auto-deriving incidents from monitoring is a core requirement and is
impossible without runtime state, so a data model is mandatory. But the user
asked for dvalin-grade discipline, which is a documented process, not just CRUD.
Shipping both gives regin an operational mental model (the methodology) and the
records that move through it (SQLite), mirroring how dvalin pairs its V-Model
skills with its `issues/` tickets. We accept the larger up-front scope and
sequence it across FEAT tickets.

## Spawned features

- **FEAT-001** — Operations methodology doc set (ITIL discipline) under `.repo/project`
- **FEAT-002** — ITIL data model in SQLite (incident / change / problem)
- **FEAT-003** — ITIL CLI verb families (`incident`, `change`, `problem`)
- **FEAT-004** — Monitoring evaluation → auto-create incidents, recurrence → problems
