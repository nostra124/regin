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

## Decision (resolved with user — guided Q&A 2026-06-19)

**Mode detection — Variant C (effective mode).** Effective mode = configured target
(bus/persona) **AND** recent reachability (last-send health). A configured-but-down
bus falls back to standalone; transient blips don't flip the mode. Adds a runtime
reachability notion + last-send health state (today mode is inferred only from persona
config).

**Q1 — Greeting scope: actionable items + a one-line health summary.** The standalone
login greeting (`regin chat` opening) shows the changes **awaiting approval** and the
**problems needing a decision**, plus a one-line health summary that **counts** open
incidents (regin handles incidents autonomously — summarize, don't dump).

**Q2 — Critical push: opt-in, critical-only, in v1.** Beyond the login greeting,
*critical* standalone items trigger an **active push** over an opt-in channel
(ntfy / webhook / email), **off by default**, configured via `regin config`.
Non-critical items still wait for the login greeting. This restores a narrow active
channel the pull-at-login model had set aside — bounded to critical severity so it
never becomes noisy.

**Q3 — On recovery: auto-flush, re-validated.** When the bus becomes reachable again,
parked items resume their intended routing automatically: each is **re-validated for
current relevance** (items that self-resolved are dropped) and then flushed to the
supervisor. Parking was only a fallback for an unreachable bus.

Distinct from **FEAT-015**: that bridge asks the *dev plane* to mint a BUG/FEAT
ticket; this escalation asks a *human/supervisor* for a **decision/approval**. They
share the bridge but carry different intents.

## Spawned features

- **Effective-mode detection** — runtime reachability probe + last-send health;
  effective mode = configured-target AND recently-reachable; exposes org-vs-standalone
  to the loop. Milestone 0.5.0.
- **Decision/approval escalation (org)** — route `pending_approval` changes + problems
  needing a decision to the supervisor over the bus; second escalation flavour atop
  the FEAT-015 bridge. Milestone 0.5.0.
- **Standalone parking + login greeting** — park items locally when standalone;
  `regin chat` opens with a health line + actionable items (pending-approval changes,
  problems). Milestone 0.5.0.
- **Critical-only active push** — opt-in (off by default), severity-gated push channel
  (ntfy / webhook / email) for critical standalone items; configured via
  `regin config`. Milestone 0.5.0.
- **Parked-item auto-flush on recovery** — on bus recovery, re-validate parked items
  for relevance and flush to the supervisor; drop self-resolved items. Milestone 0.5.0.
