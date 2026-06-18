---
id: DISC-004
type: discovery
priority: high
status: open
complexity: L
spawned_features: ~
---

# DISC-004 — Foreman mode: local worker supervision + messaging-bus client

> regin-side counterpart of dvalin **DISC-029**. dvalin owns the bus and the cave
> lifecycle; this DISC is what regin must implement to be the **cave foreman**.

## Describe

In the dvalin cave model, regin is installed in a container as `regin@cave` and
becomes the **foreman**: a 24/7, disciplined, sudo-capable agent that

1. receives **cave-level tasks** from dvalin (down the bus, as structured messages);
2. **decomposes** them with its methodology/ITIL discipline;
3. **triggers and supervises the local CLI workers** (`claude@cave`,
   `opencode@cave`) — which are pull-only tools — via its command-exec tool,
   capturing their output;
4. **collects status/governance info** from those workers and represents them
   upward (the workers are *not* on the bus — only foremen are);
5. **reports structured handovers** and emits message/step events to dvalin.

regin already has the raw materials: a persistent `regind` daemon, command-exec
+ file + web tools, scheduled skills, and (per MILESTONE-0.2.0) ITIL records +
self-improving memory. This DISC defines the **foreman runtime** and the **bus
client** on top of them.

## To explore / decide

- **Bus client** — how regind speaks the dvalin messaging protocol via execd
  (auth-stamped `role@cave` identity; push/long-poll delivery for the always-on
  foreman). Two message modes: unstructured (inform/request) and structured
  (typed JSON; work handover flows here — no separate ticket API, per DISC-029).
- **Local worker supervision** — regin relocates dvalin's stdin/stdout supervisor
  loop *into* the cave for the CLI workers: spawn `claude`/`opencode`, inject
  context, detect idle, capture results, enforce discipline/gates locally. How
  much of dvalin's supervisor semantics regin reimplements vs. reuses.
- **Local delivery** — when a structured message targets a worker, the foreman is
  the delivery/trigger agent (workers have no mailbox of their own).
- **Status/governance collection** — what the foreman aggregates about its workers
  (progress, resource use, gate state) and how it surfaces it upward.
- **Reporting/handover** — map worker outcomes into structured handover messages +
  events at cave-task granularity (observability, not babysitting).
- **Discipline boundary** — how the foreman applies regin's ITIL/methodology to
  in-cave work (e.g. a failed worker step → incident; recurring → problem;
  escalate upward as a structured message).

## Spawned features (to derive on close)

- `regind` messaging-bus client (send/inbox/subscribe via execd; identity, modes)
- Foreman mode: cave-task intake → decompose → supervise workers → handover
- Local CLI-worker supervisor (spawn/trigger/idle/capture for claude/opencode)
- Worker status/governance aggregation + upward reporting
- Structured-message ↔ ITIL bridge (incident/problem/escalation over the bus)
