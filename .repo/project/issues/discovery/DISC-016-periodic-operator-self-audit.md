---
id: DISC-016
type: discovery
priority: medium
status: open
complexity: M
spawned_features: ~
---

# DISC-016 — Periodic operator self-audit (the recurring CSI review)

## Operating-plane context

Operator plane (see DISC-008). This is the **scheduled, periodic review** that keeps
the rest of the operator loop honest — the recurring half of ITIL **CSI**, as
opposed to the continuous monitoring loop. Spawned from DISC-015 Q4 (demotion is
*one* function of this audit, not the whole of it).

## Describe

DISC-015 establishes a self-optimizing monitoring loop (LLM judgment → promotion to
cheap deterministic checks → notice filters → KPIs). A continuous loop drifts
without a periodic, wider-lens review. regin should run a **regular operator
self-audit** (cadence TBD — e.g. monthly) that steps back and checks the whole
operator plane against itself, then files its findings as the normal ITIL artefacts
(incidents/problems/changes) and tunes its own parameters.

Candidate scope (to refine with user):

1. **Re-judge promoted checks (demotion).** LLM shadow-audit of a sample of what
   each promoted deterministic check now decides; demote those that miss or
   over-fire. (The immediate-demotion path on real-world contradiction lives in
   DISC-015; this is the periodic sweep.)
2. **Review the KPIs / CSI trends.** Are cost and time-in-deviation both trending
   down? Is any service near/under the reliability floor? Is the promotion-error
   rate acceptable? Surface regressions as problems.
3. **Tune promotion criteria.** Adjust the self-adapting promotion thresholds
   (DISC-015 Q3) based on the observed promotion-error rate.
4. **Notice-filter hygiene.** Are filters dropping real signal (false-drop)? Are
   they stale? Re-tune; flag risky filters.
5. **To-be-state drift.** Has reality diverged from the declared target in ways the
   target should absorb (or vice-versa)? Propose desired-state updates as *changes*
   for human review (never silently rewrite human-authored intent — DISC-008).
6. **Coverage & toil.** Domains with no checks; high-toil incident classes that are
   promotion candidates; recurring incidents that should become problems.
7. **…and more** (per user) — the audit is intended to be the broad, reflective
   review, extensible over time.

The audit's findings flow into the existing ITIL records and the KPI store; the
audit itself should be cheap relative to what it saves (it is an LLM-heavy job, so
its own cost is a KPI input).

## Variants considered

| Variant | Summary | Key trade-off |
|---|---|---|
| A | **Scheduled self-audit skill** producing a report + ITIL artefacts | Fits the existing scheduled-skill + ITIL model; cadence is a known lever |
| B | Continuous-only (no periodic step); rely on the live loop | Cheapest; but no wide-lens review — drift and stale checks accumulate |
| C | Human-run audit (regin only gathers data) | Maximum oversight; defeats autonomy and adds standing toil |

**Leaning:** Variant **A** — a scheduled audit skill on a configurable cadence that
emits a review report and files findings as incidents/problems/changes, with the
heavier actions (e.g. desired-state changes) going through the DISC-009 approval
gate.

## Open questions (resolving with user)

1. **Cadence** — fixed (monthly) vs adaptive (more often while the system is young
   / KPIs volatile, less often once stable)?
2. **Scope confirmation** — which of 1–7 above are in v1; what is the "and more"?
3. **Output** — a single audit report (where stored / shown?) plus filed ITIL
   artefacts? Does the audit summary join the login greeting / `regin metrics`?
4. **Authority** — which audit findings does regin act on autonomously vs route
   through the DISC-009 approval gate (esp. desired-state changes, demotions)?
5. **Cost ceiling** — does the audit get a budget, and is skipping/trimming it when
   over-budget itself a tracked event?

## Decision

_Pending — to be discussed with the user (guided Q&A)._

## Spawned features

_Pending DISC close._
