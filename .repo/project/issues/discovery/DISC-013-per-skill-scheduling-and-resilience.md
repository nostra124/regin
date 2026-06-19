---
id: DISC-013
type: discovery
priority: high
status: open
complexity: M
spawned_features: ~
---

# DISC-013 — Per-skill scheduling & operator self-resilience

## Operating-plane context

Operator plane (see DISC-008). How operator skills (DISC-012) and promoted
deterministic checks (DISC-015) are **scheduled**, and how regin stays **alive and
healthy** while running unattended 24/7 (watchdog, API backoff, graceful degradation,
recovery after downtime).

## Describe

regin already ships a scheduler (cadence strings `hourly|daily|weekly|monthly|every
Xm|Xh|Xd`) and a per-user **lingering systemd service** (`daemon.enabled`). The
standalone-operator context (Mode B) needs more:

1. **Per-skill cadence.** Each operator skill has a natural rhythm (a disk monitor
   every few minutes; cert-expiry daily). Where does the cadence come from, and how is
   it tuned?
2. **Load / cost smoothing.** If every skill fires together, LLM calls (and cost)
   bunch. Operator monitoring is LLM-heavy (DISC-015), so scheduling must spread work.
3. **Resilience when the LLM is unreachable / over-budget.** The agent loop depends on
   an external LLM API. It can be down, rate-limited, or over the cost budget
   (DISC-015/016). regin must degrade gracefully, not crash or hammer the API.
4. **Recovery after downtime.** When the daemon was off (reboot, crash) and comes
   back, what happens to runs that were due?

DISC-015's **promoted deterministic checks are LLM-free** — they are the natural
degraded-mode fallback that keeps monitoring alive when the LLM is unavailable.

## Variants considered

| Point | Options | Leaning |
|---|---|---|
| Cadence source | central config only · **skill-declared default + user/config override** (+ optional per-domain tune in to-be-state) | skill-declared default + override |
| Load smoothing | none · **automatic jitter/staggering** of scheduled runs | automatic jitter |
| LLM-unavailable | crash/skip · backoff only · **exponential backoff + degrade to deterministic checks + self-incident if prolonged** | backoff + graceful degradation |
| Missed runs | replay every missed interval · **coalesced run-once catch-up** (with staleness check) · skip | coalesced run-once |
| Watchdog | bespoke supervisor · **systemd restart (already there) + internal heartbeat + self-incident on repeated skill failure** | systemd + internal heartbeat |

## Decision matrix

| Criterion | Weight | Minimal (today's scheduler) | Operator-resilient (leaning) |
|---|---|---|---|
| Survives LLM outage / rate-limit without crashing or hammering | high | ✗ | ✓ |
| Keeps monitoring alive when LLM is down (deterministic fallback) | high | ✗ | ✓ |
| Smooths cost/load (no thundering herd) | med | ✗ | ✓ |
| Sane recovery after downtime | med | ~ | ✓ |
| Reuses systemd / existing scheduler | med | ✓ | ✓ |

## Open questions (resolving with user)

1. **Cadence source** — skill-declared default + user/config override (and optional
   per-domain tune in the to-be-state doc)?
2. **LLM-unavailable / over-budget** — exponential backoff *plus* graceful degradation
   to the deterministic promoted checks, and a self-incident if the LLM stays down past
   a threshold?
3. **Missed runs** — coalesced run-once catch-up on recovery (not replay-all), with a
   staleness check?
4. **Watchdog** — rely on the systemd lingering service for process restart, and add an
   internal scheduler heartbeat + a self-incident when a skill repeatedly fails?

## Decision (resolved with user — guided Q&A 2026-06-19)

**Load smoothing — automatic jitter.** Scheduled runs are staggered with jitter to
avoid a thundering herd of LLM calls / cost spikes.

**Q1 — Cadence: skill-declared default + override.** Each operator skill declares a
default cadence; user/config overrides it; an optional per-domain tune may live in the
to-be-state doc (DISC-008). Reuses the existing cadence strings.

**Q2 — LLM outage / over-budget: backoff + degrade.** On LLM unreachable / rate-limited
/ over-budget: exponential backoff on the API **and degrade to DISC-015's LLM-free
deterministic checks** so monitoring continues; defer LLM-judgment work; raise a
**self-incident** if the LLM stays unavailable past a threshold. (Over-budget ties to
DISC-015/016 cost governance.)

**Q3 — Missed runs: coalesced run-once.** On recovery, each due skill runs **once**
(coalesced — not one run per missed interval), with a staleness check; restores current
state without a catch-up burst.

**Q4 — Watchdog: systemd + internal heartbeat.** The per-user lingering systemd service
restarts the process (already shipped); add an internal scheduler **heartbeat** and a
**self-incident** when a skill repeatedly fails. No bespoke supervisor.

## Spawned features

- **Per-skill scheduling** — skill-declared default cadence + user/config override (+
  optional to-be-state per-domain tune); automatic jitter/staggering of runs.
  Milestone 0.5.0.
- **LLM-resilience + graceful degradation** — exponential backoff on API failure;
  degrade to deterministic promoted checks (DISC-015) during an outage; self-incident
  on prolonged unavailability; respects the cost budget (DISC-015/016). Milestone 0.5.0.
- **Downtime recovery (coalesced catch-up)** — on daemon restart, run each due skill
  once with a staleness check. Milestone 0.5.0.
- **Liveness watchdog** — internal scheduler heartbeat + self-incident on repeated
  skill failure, atop the systemd lingering service. Milestone 0.5.0.
