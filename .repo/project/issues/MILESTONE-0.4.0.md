---
status: done
phase: alpha
---

# v0.4.0 — Governance & continuity participation

regin participates in the organization's governance and resilience: it **chairs
and attends meetings** (producing minutes), runs its **individual planning cycle**
(feeding the org plan), and acts as a **deputy** for business continuity. This is
regin's half of dvalin MILESTONE-1.4.0 (governance) and 1.5.0 (continuity).

## Source discovery
- **DISC-006** — regin's individual planning cycle (aggregate per-repo When/Which → plan; emit upward).
- **DISC-004** — meeting-chair behaviour (run agenda → minutes over the bus).
- **DISC-007** / **DISC-037** (dvalin) — deputy participation: hold a role's skill
  package + continuity brief, attend meetings as observer, take over on failover.

## Scope (FEATs derived on DISC close)
- regin per-agent planning routine (weekly/quarterly/yearly); upward signals (priority asks, capability gaps).
- Meeting-chair: run the standard agenda, collect reports, emit minutes + action-items.
- Deputy mode: standing-brief + observer attendance; activate on supervisor-confirmed failover; hand back.

## Depends on
- dvalin **MILESTONE-1.4.0** (meetings/planning) + **1.5.0** (deputy/continuity). Cross-repo.

## Out of scope
- A `ROADMAP.md` (the roadmap is the collective `MILESTONE-*.md` files).

## Tickets (derived from DISC-004/006/007/037)

| FEAT | Title | depends_on | Status |
|------|-------|-----------|--------|
| FEAT-016 | Meeting-chair: agenda → minutes + action-items over the bus | 010 | **done** |
| FEAT-017 | Individual planning cycle (When/Which → plan; emit upward) | 010 | **done** |
| FEAT-018 | Deputy mode: continuity brief + observer + failover | 011, 014 | **done** |

Order: 016/017/018 independent.
