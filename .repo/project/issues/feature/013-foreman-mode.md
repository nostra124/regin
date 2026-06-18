---
id: FEAT-013
type: feature
status: open
milestone: 0.3.0
disc: DISC-004
---
# FEAT-013 — Foreman mode: cave-task intake → decompose → supervise → handover

Ties the bus client (010) + worker supervisor (012). A structured cave-task
message arrives in the inbox; the foreman parses it, runs the worker, and posts
a structured **handover** message back up the bus with the outcome. A failed
worker step opens an ITIL incident (discipline boundary).
- `foreman::handle_task(msg)` → worker run → handover message (+ incident on
  failure).
- CLI `regin foreman run-once` (drain inbox, handle cave-tasks).

Acceptance: a structured cave-task yields a handover message with the worker
outcome; a failed worker also opens an incident. Unit-tested with a stub worker.
