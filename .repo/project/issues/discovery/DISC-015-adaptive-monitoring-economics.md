---
id: DISC-015
type: discovery
priority: high
status: open
complexity: L
spawned_features: ~
---

# DISC-015 — Adaptive monitoring economics (LLM-judged triage that self-optimizes toward cheap deterministic checks)

## Operating-plane context

Operator plane (see DISC-008). This is the **ITIL CSI** (Continual Service
Improvement) discipline applied to regin's *own* monitoring: regin runs a
measurable loop that drives the cost of staying at the to-be state down over time
while raising service availability/reliability. Spawned out of DISC-008 Q4.

## Guiding principle — "senseful full automation"

The north star (René's professor's phrasing): the goal is **senseful full
automation** — automate *fully*, but only *where it makes sense*. Intelligence
(LLM judgment) is expensive; deterministic automation is, above a threshold,
cheaper. regin's job is to continuously find the right balance: use LLM judgment to
*discover* what good/bad looks like, then *promote* the crystal-clear cases into
cheap automatic checks, keeping the LLM for the genuinely ambiguous. **This
principle belongs in regin's operator-plane system prompt** (a baseline operator
directive, alongside `regin-core/src/context.rs` / the operator persona).

## Describe

DISC-008 Q4 settled that *deviation → incident is LLM-judged* (not every monitoring
event is an incident). But LLM judgment per evaluation is expensive, and naïvely
running it on every log line every minute is wasteful. Pure deterministic checks
are cheap but brittle and cannot judge "is this worth an incident *relative to
intent*". Neither extreme is right. regin needs a self-optimizing framework that:

1. **Two evaluation tiers.**
   - **LLM review** — periodic (e.g. daily) review of logs/observations that
     *judges*, against the three-layer to-be state, whether a deviation is
     incident-worthy. This is the discovery/decision tier.
   - **Deterministic checks** — cheap, frequent (e.g. hourly) assertions for cases
     that are crystal clear. This is the automation tier.

2. **Promotion loop (the self-optimization).** When the LLM repeatedly reaches the
   same clear-cut verdict for a pattern, regin distills it into a deterministic
   check (a structured assertion + schedule) and thereafter relies on the cheap
   check instead of the LLM. Promoted checks naturally land in the **structured
   layer** of the to-be state (DISC-008) and their cadence is a **scheduling**
   concern (DISC-013).

3. **Notice filters.** Per-log noise filters that pre-filter log lines *before* they
   reach the LLM, cutting tokens/cost. regin builds and maintains these.

4. **Demotion / safety.** If a promoted deterministic check starts mis-firing
   (false confidence, missed real deviations), regin must fall back to LLM judgment
   (demote). Promotion is reversible.

5. **Measurability (the CSI metrics).** regin tracks and optimizes against:
   - **cost** — tokens / spend per period (NanoGPT API),
   - **coverage & precision** — incident precision/recall, missed deviations,
   - **service outcomes** — availability/reliability of the monitored services,
   - **problem rate** — chronic problems over time.
   The optimization target: **reduce cost over time while holding or raising
   reliability and lowering the problem count.** regin should derive this framework
   itself and prove the trend with numbers.

## Variants considered

| Variant | Summary | Key trade-off |
|---|---|---|
| A | LLM-only triage (today's direction, no promotion) | Best judgment; cost grows unbounded with log volume |
| B | Static two-tier (human pre-classifies cheap vs LLM checks) | Predictable cost; no learning, drifts, manual upkeep |
| C | **Self-optimizing two-tier** — LLM discovers, regin promotes/demotes, all measured | Realizes "senseful full automation"; most plumbing + needs metrics + guardrails on auto-promotion |

**Leaning:** Variant **C**, staged — ship the two tiers + metrics first, then the
promotion loop, then notice filters and auto-demotion. Promotion may start
human-approved and graduate to autonomous once the metrics are trustworthy.

## Decision matrix

| Criterion | Weight | A | B | C |
|---|---|---|---|---|
| Honours "senseful full automation" | high | ~ | ✗ | ✓ |
| Bounds/reduces cost over time | high | ✗ | ~ | ✓ |
| Keeps judgment for ambiguous cases | high | ✓ | ✗ | ✓ |
| Implementation cost now | med | ✓ | ~ | ✗ |
| Measurable (provable trend) | med | ✗ | ~ | ✓ |

## Open questions (resolving with user)

1. **Metrics surface** — where do cost/coverage/reliability metrics live and how
   are they shown? (A CSI section in the `regin chat` login greeting? a `regin
   metrics` view? both?) Stored in SQLite alongside ITIL records?
2. **Promotion trigger** — how many consistent LLM verdicts (and what confidence)
   before a pattern is promoted to a deterministic check? Human-approved promotion
   first, or autonomous-with-audit from the start?
3. **Notice-filter representation** — regex/rule files per log, or learned filters?
   Where stored — the `desired/` files, the skill, or a dedicated filters store?
4. **Demotion signal** — how does regin detect a promoted check has gone stale or
   wrong, and revert to LLM judgment?
5. **Boundary with DISC-013 (scheduling) and DISC-008 (structured layer)** — does a
   promoted check write directly into the structured to-be-state file, or into a
   separate "derived checks" store that references it?

## Decision

_Pending — being resolved with the user (guided Q&A)._

## Spawned features

_Pending DISC close._
