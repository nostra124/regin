---
id: FEAT-055
type: feature
priority: medium
complexity: L
estimate_tokens: 70k-120k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-016
depends_on: FEAT-051
---

# FEAT-055 — Scheduled operator self-audit (CSI review)

## Description
**As** regin
**I want** a periodic wide-lens self-audit
**So that** the continuous monitoring loop doesn't drift — promoted checks, KPIs,
filters, and to-be-state stay honest.

## Implementation
- A scheduled audit skill on an **adaptive cadence** (monthly default; more often while
  young / KPIs volatile, less once stable).
- **Full CSI sweep**, each function modular (activates as its dependency lands):
  1. demotion — re-judge a sample of promoted checks (FEAT-051);
  2. KPI / CSI trend review (FEAT-050) — surface regressions as problems;
  3. tune promotion criteria by observed promotion-error rate;
  4. notice-filter hygiene (false-drop / staleness, FEAT-052);
  5. to-be-state drift — propose desired-state updates as **changes for human review**
     (never silently rewrite human intent, FEAT-033);
  6. coverage & toil — domains with no checks, high-toil incident classes.
- **Output:** a stored audit report + findings filed as ITIL artefacts; a summary joins
  `regin metrics` + the login greeting.
- **Authority:** actions flow through the DISC-009 lanes (FEAT-037); to-be-state edits
  **always** require approval.
- **Cost governance:** a per-run budget; when over, trim scope (sample less) or defer,
  recording the skip/trim as a tracked event (the audit's own cost is a KPI input).

## Acceptance Criteria
1. The audit runs on its adaptive cadence, performs the available sweep functions, and
   emits a report + ITIL findings.
2. Demotions/criteria-tuning/filter-tuning apply via DISC-009 lanes; to-be-state
   proposals always route to approval.
3. Over-budget runs trim/defer and record the event; a summary appears in
   metrics/greeting; unit-tested with fakes.
