---
id: FEAT-067
type: feature
priority: high
complexity: M
estimate_tokens: 60k-100k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-010
---

# FEAT-067 — Event bus + triggers

## Description
**As** regin
**I want** a small event bus with triggers
**So that** events (incl. external ones) can start or advance plans — the bridge
between the reactive and proactive planes.

## Implementation
- A small **internal typed event bus**: `incident.created`, `objective.breached`,
  `deviation.detected`, `goal.created`, `schedule.tick`, `task.completed`,
  `task.failed`, … (publishers across the operator + intent planes).
- **External ingestion**: map inbound dvalin messages (FEAT-010 bus) into internal
  events so external triggers work.
- **Triggers** bind an event (+ optional condition) → an action: instantiate a
  task/plan, or satisfy an `event→task` dependency (FEAT-063).
- Fail-safe: a bad trigger is logged and never stalls the bus.

## Acceptance Criteria
1. Publishing an event invokes bound triggers; an event→task dependency is satisfied
   when its event fires.
2. An inbound dvalin message is ingested as an internal event and can trigger a plan.
3. A trigger error is isolated (logged, bus continues); unit-tested.
