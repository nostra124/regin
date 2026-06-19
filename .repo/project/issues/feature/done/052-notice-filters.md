---
id: FEAT-052
type: feature
priority: medium
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-015
depends_on: FEAT-049
---

# FEAT-052 — Notice filters

## Description
**As** regin
**I want** to cut known-noise before it reaches the LLM
**So that** evaluation cost drops without losing real signal.

## Implementation
- **regin-managed, hand-editable rule files** in a dedicated **filters store**
  (separate from `desired/`); user-editable, layered like skills.
- Filters drop/coalesce known-noise observations before the LLM review tier
  (FEAT-049).
- Measured by **notice-filter savings** (FEAT-050); false-drop risk is reviewed by the
  periodic self-audit (FEAT-055).

## Acceptance Criteria
1. A matching observation is filtered before reaching the LLM tier; a non-matching one
   passes through.
2. Filters are hand-editable rule files in their own store, layered user-over-system.
3. Notice-filter savings are recorded as a KPI; unit-tested.
