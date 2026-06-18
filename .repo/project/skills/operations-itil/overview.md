# Operations methodology — ITIL (regin)

regin is the **operations** counterpart to dvalin's development methodology: it
runs and maintains systems with a small, disciplined ITIL process. Where dvalin's
V-Model governs *building* software, this governs *operating* it.

This doc set is the **discipline**; the running implementation is the
`regin incident | change | problem` verbs and the monitoring evaluator
(MILESTONE-0.2.0, DISC-001). Keep the two in sync — these docs describe how an
operator (human or agent) should *use* the tools, not duplicate their reference.

## The three records

| Record | What it is | Verb |
|---|---|---|
| **Incident** | an unplanned interruption or degradation — something is wrong *now* | `regin incident …` |
| **Change** | a deliberate modification to a system — documented before/after | `regin change …` |
| **Problem** | the underlying cause behind one or more (usually recurring) incidents | `regin problem …` |

## The operating loop

```
monitor (scheduled tasks) ──▶ evaluate ──▶ incident ──▶ (recurs) ──▶ problem
                                                │                        │
                                            remediate ◀── change ◀── root cause
```

1. A scheduled task (monitor) runs and its result is **evaluated** (see
   `monitoring-triage.md`).
2. A failing/anomalous result opens an **incident**.
3. Repeated incidents of the same shape surface a **problem** (root cause).
4. A **change** remediates the incident/problem; its before/after is recorded.
5. The incident is **resolved** and **closed**; the problem is closed when the
   root cause is fixed (a *known error* in between).

## Storage

All records live in regin's SQLite store (no files). Per-repo context/memories
are keyed by repo path (FEAT-008). See `incident.md`, `change.md`, `problem.md`,
and `monitoring-triage.md` for each lifecycle.
