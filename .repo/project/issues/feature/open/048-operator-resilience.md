---
id: FEAT-048
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-013
depends_on: FEAT-047
---

# FEAT-048 — Operator resilience (degradation, recovery, watchdog)

## Description
**As** regin running unattended 24/7
**I want** to survive LLM outages, downtime, and internal failure
**So that** monitoring stays alive and recovers cleanly without crashing or hammering
the API.

## Implementation
- **LLM outage / rate-limit / over-budget:** exponential backoff on the API **and
  degrade to the LLM-free deterministic checks** (FEAT-051) so monitoring continues;
  defer LLM-judgment work; raise a **self-incident** if the LLM stays unavailable past
  a threshold. (Over-budget ties to the KPI cost governance, FEAT-050.)
- **Downtime recovery:** on daemon restart, run each *due* skill **once** (coalesced —
  not one run per missed interval), with a staleness check.
- **Liveness watchdog:** rely on the per-user lingering systemd service for process
  restart (already shipped); add an internal scheduler **heartbeat** + a
  **self-incident** when a skill repeatedly fails.

## Acceptance Criteria
1. With the LLM unreachable, deterministic checks keep running and the API is retried
   with exponential backoff; a prolonged outage raises a self-incident.
2. After downtime, due skills run once (coalesced), not once per missed interval.
3. Repeated skill failure raises a self-incident; the heartbeat detects a stalled
   scheduler; unit-tested with fakes.
