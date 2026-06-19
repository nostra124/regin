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

5. **Measurability — the KPI framework.** regin tracks a bounded KPI set (and may
   extend it over time). Two framing rules first:
   - **Reliability is a constraint, not a currency.** The objective is *minimize
     cost **subject to** reliability/precision staying at or above target*. Cost may
     never be bought at the expense of the to-be state — otherwise a naïve
     cost-minimizer "optimizes" by judging less and missing deviations.
   - **North-star pairing:** *cost per period* ↓ **while** *time-in-deviation* ↓
     (cumulative time reality ≠ the to-be state). Both falling together is the proof
     the framework works.

   | Group | KPI | Why it steers |
   |---|---|---|
   | Economics | LLM spend & tokens / period; cost per evaluation; cost per incident caught | the spend being optimized |
   | Economics | **Automation ratio** — % of checks served by the cheap deterministic tier vs the LLM tier | the direct measure of "senseful full automation" progress |
   | Economics | **Notice-filter savings** — % log volume / tokens filtered before the LLM | the main cost lever |
   | Economics | **Cost avoided** — counterfactual LLM cost displaced by promoted checks | proves promotion pays |
   | Detection | Incident **precision** (true / raised → alert-noise) & **recall** (caught / actual → escape rate) | not too noisy, not blind |
   | Detection | **MTTD** — mean time to detect a deviation | how fast off-target is seen |
   | Reliability | **Time-in-deviation** + per-service availability | the actual outcome |
   | Reliability | **MTTR** — mean time to restore to target; change success & rollback rate | how fast/cleanly we recover (ties DISC-009) |
   | Reliability | Incident **recurrence rate** → problem rate / open-problem aging | chronic-issue pressure |
   | Self-optim. | **Promotion error rate** — promoted checks later demoted; demotion latency | the self-optimizer must not silently degrade reliability |
   | Autonomy | **Autonomy ratio** — % incidents auto-remediated vs escalated to a human | toil saved (ties DISC-009/010) |

   regin derives and maintains this itself, and must show the **trends** (slopes),
   not just point values. Explicitly *out* of the steering set (drill-downs, not
   KPIs): raw event counts, per-domain dashboards, per-skill token breakdowns.

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

1. **Objective + v1 KPI core** — is the constrained objective (minimise cost s.t.
   reliability/precision ≥ target) the right framing? Which KPIs are v1-core vs
   deferred?
2. **Metrics surface** — where do the KPIs live and how are they shown? (A CSI
   section in the `regin chat` login greeting? a `regin metrics` view? both?)
   Stored in SQLite alongside ITIL records?
3. **Promotion trigger** — how many consistent LLM verdicts (and what confidence)
   before a pattern is promoted to a deterministic check? Human-approved promotion
   first, or autonomous-with-audit from the start?
4. **Notice-filter representation** — regex/rule files per log, or learned filters?
   Where stored — the `desired/` files, the skill, or a dedicated filters store?
5. **Demotion signal** — how does regin detect a promoted check has gone stale or
   wrong, and revert to LLM judgment?
6. **Boundary with DISC-013 (scheduling) and DISC-008 (structured layer)** — does a
   promoted check write directly into the structured to-be-state file, or into a
   separate "derived checks" store that references it?

## Decision (resolving with user — guided Q&A 2026-06-19)

**Q1a — Objective: constrained.** Minimise cost **subject to** reliability/precision
≥ a target floor. Reliability is never traded for cost; the optimizer only banks
savings in the slack above the floor.

**Q1b — v1 KPI scope: the full set.** All four groups ship in v1 — cost +
automation ratio, time-in-deviation + availability, precision/recall + MTTD/MTTR,
promotion-error + autonomy ratio. Caveat recorded: the **KPI schema is fully
defined and stored in v1**, but the promotion-error and autonomy KPIs only begin
**reporting** once their underlying features exist (promotion loop; auto-remediation
per DISC-009) — v1 is not forced to build the whole promotion+remediation stack
merely to populate a metric.

**Q2 — Metrics surface: greeting + command.** KPIs are stored in SQLite alongside
the ITIL records. A short CSI summary (north-star trend + anything breaching the
reliability floor) appears in the `regin chat` login greeting (DISC-010); a full
`regin metrics` command exposes the trends/history on demand.

**Q3 — Promotion: autonomous, with regin-owned criteria.** regin promotes a
recurring verdict into a deterministic check **autonomously**, logging every
promotion to a reviewable/revertible audit trail. The promotion **criteria are
regin's own and self-adapting**, but grounded in *both* inputs: (a) N consistent
clear-cut verdicts + high LLM confidence, and (b) a statistical error-bound on the
candidate check. The **promotion-error KPI** governs the criteria and **demotion**
is the safety net, so self-tuning cannot silently erode reliability (consistent
with the constrained objective).

**Q4 — Demotion: via a periodic self-audit (→ DISC-016).** Stale/wrong promoted
checks are caught by a **regular operator self-audit** (e.g. monthly) that re-judges
what promoted checks now decide (LLM shadow-audit style) and demotes those that
miss/over-fire — *plus* real-world contradictions (a later incident, a human
override) demote immediately between audits. The audit is broader than demotion
(it also reviews KPIs, promotion criteria, drift, and more) and is **spun out to
its own discovery — DISC-016 (periodic operator self-audit)**.

**Q5 — Notice filters: regin-managed rule files.** Human-readable regex/match rules
in a **dedicated filters store** (sibling to the `desired/` files), authored and
maintained by regin, hand-editable and reviewable — kept separate from the
desired-state so "what good looks like" never mixes with "what to ignore".

**Q6 — Promoted checks: a separate derived-checks store.** Promoted deterministic
checks live in their own **machine-managed derived-checks store** that *references*
the to-be-state, distinct from the human-authored `desired/` structured layer
(DISC-008). This keeps a clean line between human-declared intent and
regin-synthesised checks; the scheduler (DISC-013) runs them on their promoted
cadence.

All six questions resolved. Demotion detail depends on **DISC-016**.

## Spawned features (to derive on close)

- **Two-tier evaluation engine** — periodic LLM review tier (judges
  deviation-worth against the three-layer to-be state) + a cheap deterministic
  check tier; both feed the incident/problem flow.
- **KPI store + `regin metrics`** — full KPI schema (all four groups) in SQLite
  alongside ITIL records, constrained-objective evaluation (cost s.t. reliability ≥
  floor), trend tracking; CSI summary in the login greeting (DISC-010) + a
  `regin metrics` command. Promotion-error/autonomy KPIs report once their features
  land.
- **Promotion loop** — autonomous, audited promotion of stable LLM verdicts into a
  separate **derived-checks store**; regin-owned, self-adapting criteria grounded
  in N-consistent+confidence and a statistical error-bound; governed by the
  promotion-error KPI.
- **Notice filters** — regin-managed, hand-editable rule files in a dedicated
  filters store; measured by notice-filter savings.
- **Demotion hook** — immediate demotion on real-world contradiction/override; the
  periodic re-audit lives in **DISC-016**.
- **"Senseful full automation" operator directive** added to regin's operator-plane
  system prompt.
