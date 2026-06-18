---
id: FEAT-004
type: feature
priority: high
complexity: L
estimate_tokens: 60k-110k
estimate_time: 90-150min
phase: open
status: open
depends_on: FEAT-002
spawned_from: DISC-001
---

# Monitoring evaluation → auto-create incidents; recurrence → problems

## Description
**As** regin
**I want** scheduled task-run results to be evaluated, with failing/anomalous
runs auto-opening incidents and recurring incidents surfacing problems
**So that** monitoring produces actionable operational records, not just logs.

This is the core "monitoring results should be evaluated and potential incidents
or problems created" requirement.

## Implementation
- After a scheduled task run completes in `regind`, evaluate its result:
  - Deterministic first pass: non-success status / error markers → candidate
    incident.
  - Optional LLM judgement (bounded, configurable) to classify severity and
    decide whether an anomalous-but-zero-exit run still warrants an incident.
- De-duplicate: a still-open incident for the same skill+signature is updated
  (occurrence count / timestamp) rather than duplicated.
- **Recurrence → problem**: when N incidents of the same signature occur within a
  window (configurable threshold), auto-open (or link to) a problem and attach
  the incidents (`problem_incidents`).
- Settings (SQLite, via existing `config`): `monitor.auto_incident` (bool),
  `monitor.severity_model` (use LLM or deterministic), `problem.recurrence_threshold`,
  `problem.recurrence_window`.
- Emit episodic-memory entries for each evaluation (ties into FEAT-005).

## Acceptance Criteria
1. A scheduled run that fails opens exactly one incident; a second failure of the
   same signature updates that incident, not a duplicate.
2. Reaching the recurrence threshold opens/links a problem with the contributing
   incidents attached.
3. Auto-creation is gated by `monitor.auto_incident`; off by default until set.
4. Evaluation never blocks or crashes the scheduler loop on LLM/parse errors
   (fails safe, logs, continues).
5. Unit tests cover the dedupe and recurrence-threshold logic with a fake clock.
