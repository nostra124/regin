---
id: FEAT-061
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-060
---

# FEAT-061 — Goal model + store (achieve a state by a date)

## Description
**As** regin
**I want** dated goals with derived success criteria
**So that** I can drive toward "achieve X by date D" and know when it's done.

## Implementation
- New `goals` store: id, description (LLM text), **target** + **deadline**, derived
  **success criteria**, **priority**, **source** (dvalin-LLM / human / regin),
  **RAG health**, lifecycle `proposed → active → achieved | failed | abandoned`,
  timestamps.
- **Success criteria derived at planning time** — measurable/structural preferred
  (a to-be-state snapshot that must hold), LLM-judged only where measurement is too
  fuzzy (the measurable-preferred / LLM-fallback rule).
- **Done-detection**: achieved when criteria hold before the deadline; auto-failed if
  the deadline passes unmet (LLM fallback for fuzzy criteria).

## Acceptance Criteria
1. A goal round-trips with target/deadline/derived criteria/priority/source and
   moves through its lifecycle states.
2. Measurable criteria auto-detect achievement; a fuzzy goal falls back to LLM
   judgement (injectable in tests).
3. A deadline passing with criteria unmet auto-fails the goal; unit-tested with a
   fake clock.
