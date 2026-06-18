# Monitoring triage — runs to incidents and problems

Scheduled tasks (monitors) produce results that must be **evaluated**, not just
logged. This is regin's "monitoring results are evaluated → incidents/problems"
behaviour (FEAT-004).

## How a run becomes an incident

After each scheduled run, if `monitor.auto_incident = true`:

1. **Success** → no-op.
2. **Failure / error** → an incident is opened for the skill, at
   `monitor.severity` (default `medium`), `source = monitor`.
3. **De-duplication** — while an incident for that skill is still
   `open`/`investigating`, a further failure **updates that incident** rather
   than opening a duplicate.

An episodic-memory entry is recorded whenever an incident is opened (feeds the
self-improving memory loop).

## How recurrence becomes a problem

When the number of incidents recorded for one skill reaches
`monitor.recurrence_threshold` (default `3`), a **problem** is opened (or an
existing one reused) and the contributing incidents are linked. Recurrence
accumulates as incidents are closed and re-open over time.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `monitor.auto_incident` | `false` | gate — turn on to auto-derive incidents |
| `monitor.severity` | `medium` | severity for auto-opened incidents |
| `monitor.recurrence_threshold` | `3` | incidents of one skill before a problem opens |

Evaluation **fails safe**: an error in triage is logged and never breaks the
scheduler loop.

## Signature

The current "shape" of an incident is the **skill name** (one signature per
skill). A future refinement may fingerprint the error text for finer grouping.
