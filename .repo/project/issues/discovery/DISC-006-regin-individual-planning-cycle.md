---
id: DISC-006
type: discovery
priority: medium
status: open
spawned_features: ~
complexity: M
---

# DISC-006 — regin's individual planning cycle

> regin-side counterpart of dvalin **DISC-035** (org planning cycle). dvalin's
> **CSO** runs the *org-wide* planning; this DISC is each regin planning **its own
> domain/cave** and feeding signals upward.

## Describe

The org's *When* (task timing) and *Which* (capabilities/tools) live
**decentralized in the repos** — and for regin, concretely in its **XDG store
keyed by repo path** (FEAT-008): per-repo scheduled tasks (*When*) and per-repo
required skills/capabilities (*Which*). Each regin must **plan its own work** from
these, on a recurring cadence, and feed the result up both matrix axes
(DISC-032): delivery/priorities sideways to its project/process owner, capability
gaps up the functional line to CAO.

## To explore / decide

- **What regin's individual plan contains** — its backlog (ITIL incidents/
  problems/changes + scheduled tasks), per-repo capability needs, and its
  self-improvement plan (Hermes: which memories/skills to consolidate next).
- **Cadence** — weekly (operational backlog), monthly (capability + Hermes
  review), yearly (rolls up into the org strategic cycle). Aligns with DISC-035.
- **Aggregation source** — read per-repo *When/Which* from the FEAT-008 store +
  scheduled tasks + ITIL records; summarize into the plan.
- **Upward signals** — emit, as structured messages (dvalin DISC-029): schedule/
  priority asks to the project/process owner; **capability gaps** to CAO; risks/
  escalations to the functional line.
- **Relationship to Hermes (DISC-002)** — self-improvement is part of the plan:
  the monthly review decides what the agent should get better at next.

## Spawned features (to derive on close)

- regin per-agent planning routine (weekly/monthly/yearly)
- Aggregate per-repo When/Which (FEAT-008) + scheduled tasks + ITIL backlog
- Emit upward signals (priority asks, capability gaps) as structured messages
- Tie the plan to Hermes self-improvement targets
