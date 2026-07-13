---
id: FEAT-062
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.7.0
spawned_from: DISC-019
depends_on: FEAT-061
---

# FEAT-062 — Intent dependency & conflict graph

## Description
**As** regin
**I want** to know how goals/objectives relate
**So that** achieving one that advances or undermines another is handled, not blind.

## Implementation
- A relation store over goals+objectives: **`supports`** (achieving X advances /
  achieves Y) and **`conflicts_with`** (X works against Y).
- **Conflict detection**: surface pairs whose pursuit pulls apart (e.g. a goal whose
  plan would breach an objective, or two `conflicts_with` intents both active).
- **Arbitration by priority**: the higher-priority intent wins; the lower is
  deferred/adjusted, with a recorded **mitigation** where both must coexist.
- `supports` propagation: achieving X may auto-advance a supported Y's progress.

## Acceptance Criteria
1. `supports` / `conflicts_with` relations persist and are queryable both directions.
2. Two active `conflicts_with` intents are detected; arbitration selects by priority
   and records a mitigation for the deferred one.
3. Achieving a supporting intent advances the supported intent's progress;
   unit-tested.
