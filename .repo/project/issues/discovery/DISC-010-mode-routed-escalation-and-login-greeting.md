---
id: DISC-010
type: discovery
priority: high
status: open
complexity: M
spawned_features: ~
---

# DISC-010 — Mode-routed escalation and the standalone login greeting

## Operating-plane context

Operator plane (see DISC-008). Concerns how regin reaches a human/supervisor when
a fix needs a decision, approval, or help — and how that differs by mode.

## Describe

When a change is `pending_approval` (DISC-009) or a problem needs a
decision/help, regin must route it. The destination depends on regin's **runtime
mode**:

- **In a dvalin org (foreman/bus reachable):** hand the item to the supervisor
  over the bus. Note this is a *second flavour* of escalation distinct from
  FEAT-015, which only asks the dev plane to mint a BUG/FEAT ticket. This one asks
  a human/supervisor for a **decision/approval**.
- **Standalone (no dvalin reachable):** there is no live channel to push to.
  Instead, items are **parked locally** and surfaced at the next **human login**:
  the user runs `regin chat`, and regin opens with a greeting — current health
  status + the problems/changes it needs help with. This replaces an
  active push channel (SMTP/webhook/matrix) with a pull-at-login model.

regin currently has **no runtime notion of "is dvalin reachable"** (mode is
inferred only from whether a persona is configured), and the escalation bridge
has **no offline fallback** (`escalation.rs` is a pure payload builder). Both are
gaps this DISC fills.

## Variants considered (mode detection)

| Variant | Summary | Key trade-off |
|---|---|---|
| A | Static — bus/persona configured ⇒ org, else standalone | Trivial; wrong when bus is configured but *down* |
| B | Runtime reachability probe of the bus/execd | Accurate; needs health-check + state |
| C | Both — static config + last-send health (effective mode) | Robust; a little more state to track |

## Decision matrix

| Criterion | Weight | A | B | C |
|---|---|---|---|---|
| Correct when dvalin is configured but down | high | ✗ | ✓ | ✓ |
| Simplicity | med | ✓ | ~ | ~ |
| No false "standalone" on transient blips | med | ✓ | ~ | ✓ |

**Leaning:** Variant **C** — effective mode = configured-target AND recent
reachability. Standalone baseline = login greeting; an optional active push for
*critical* items is a possible later addition (open question 2).

## Open questions (resolving with user)

1. Login greeting scope: problems only, or problems + open incidents + a one-line
   health summary?
2. Do truly *critical* standalone items ever warrant an active push (email/ntfy),
   or is login-greeting always sufficient?
3. When dvalin recovers, do parked items auto-flush up the bus, or stay parked for
   the human to decide?

## Decision

_Pending — being resolved with the user (guided Q&A)._

## Spawned features

_Pending DISC close._
