---
id: FEAT-018
type: feature
status: open
milestone: 0.4.0
disc: DISC-007
---
# FEAT-018 — Deputy mode: continuity brief + observer + failover

regin acts as a deputy for business continuity (dvalin DISC-037): it holds a
role's skill package + a standing continuity brief (never the primary's private
memory), attends the role's meetings as observer, and takes over on
supervisor-confirmed failover, handing back when the primary returns.

- deputy::DeputyState (standby|active) + transitions: activate (on confirmed
  failover) / handback; refuse activate without confirmation.
- continuity brief store (current state/policies/open items), updated by primary.
- CLI `regin deputy show|activate|handback`.

Acceptance: deputy starts standby; activate requires a confirmation flag; handback
returns to standby; the brief round-trips. Unit-tested.
