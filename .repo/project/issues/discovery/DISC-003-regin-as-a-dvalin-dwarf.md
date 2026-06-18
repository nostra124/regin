---
id: DISC-003
type: discovery
priority: medium
status: open
complexity: L
spawned_features: ~
---

# DISC-003 — Regin as a dvalin dwarf (workflow-engine integration)

> Forward-looking analysis + recommendation. No implementation in MILESTONE-0.2.0.
> Closing this DISC will spawn FEATs in **both** regin and dvalin.

## Describe

dvalin and regin are two planes of the same loop:

- **dvalin = development plane.** A pure Rust, **no-LLM** workflow engine.
  Durable SQLite runs, typed steps dispatched to **out-of-process executor
  plugins** (`dvalin-execd`), events/waits, a generic scheduler, a **dwarf pool
  + allocator** with **capability matching / platform routing**, a **handover
  protocol**, and a **JSON API for inter-agent ticket submission**. All AI work
  is delegated to **dwarfs** — coding agents (Claude, opencode) in podman
  containers, driven by a stdin/stdout supervision relay. Its dev methodology is
  the `milestone-cycle` workflow (`conduct ≡ run milestone-cycle`).

- **regin = operations plane.** An LLM-backed agent (NanoGPT) with real tools
  (command exec, file r/w, web search), a `regind` daemon, a Unix-socket
  `Request`/`Response` protocol, scheduled skills, and — after MILESTONE-0.2.0 —
  ITIL incident/change/problem records and self-improving memory.

The goal: regin acts as **one of dvalin's dwarfs** so the loop closes — ops
detects and frames work, dev executes it, ops verifies and records the change:

```
regin monitors → incident → recurrence → problem (root cause)
      │ escalate (needs a code/config change)
      ▼
dvalin engine runs milestone-cycle → dwarf develops the fix → release
      │ deploy
      ▼
regin verifies resolution → records a Change → closes the incident/problem
```

The question is **how** regin plugs into dvalin's engine. dvalin already exposes
three seams we can target.

## Variants considered

| Variant | Seam in dvalin | What regin implements | Coupling |
|---|---|---|---|
| A | **JSON API — inter-agent ticket submission** | regin escalates problems as BUG/FEAT tickets into dvalin's backlog via its JSON API | Loose |
| B | **Out-of-process executor plugin** (`execd` step-type) | regin advertises step types (e.g. `ops.remediate`, `monitor.verify`, `incident.triage`) the engine dispatches to it | Medium |
| C | **Dwarf-pool member** (supervised agent) | regin registers as a dwarf identity with a capability profile; the allocator assigns it steps; regin honours the **dwarf step contract** + **handover protocol** | Tight |

These are **complementary layers**, not exclusive — A is the escalation bridge,
B makes regin a typed worker the engine can call, C makes regin a first-class
agent that can take dev/ops steps end-to-end.

## Decision matrix

| Criterion | Weight | A | B | C |
|---|---|---|---|---|
| Time-to-first-value | high | ✓ | ~ | ✗ |
| Closes the ops→dev→ops loop | high | ✓ | ✓ | ✓ |
| Reuses dvalin's engine-first seams as intended | high | ~ | ✓ | ✓ |
| Lets regin actually *develop* software | med | ✗ | ~ | ✓ |
| Honours dvalin's no-LLM invariant (LLM stays in the dwarf) | high | ✓ | ✓ | ✓ |
| Implementation cost | med | low | med | high |

## Arguments

### Pro — sequence A → B → C

- **A first (escalation bridge).** Highest value for least cost and the cleanest
  ownership boundary: regin's problem management is exactly the place where "this
  needs a code change" is decided. regin maps a `problem` → a dvalin BUG/FEAT via
  the JSON API, then watches for the fix's release to verify and close. No change
  to dvalin's execution model; respects the no-LLM invariant (regin's LLM stays
  on regin's side of the wire).
- **B next (executor plugin).** dvalin's engine is explicitly built to dispatch
  typed steps to out-of-process executors that *advertise step-type plugins*.
  regin is a perfect such executor for **ops-shaped steps** a dev workflow needs
  — `monitor.verify` (confirm a deploy fixed the incident), `ops.remediate`
  (apply a runbook), `incident.triage`. The engine matches steps to regin by
  capability/platform routing; regin runs them with its tools and returns typed
  step I/O + deliverables. This makes regin a reusable engine citizen, not just a
  ticket source.
- **C last (dwarf-pool member).** Because regin already has an LLM + tools, it can
  be a *dwarf* that takes development or maintenance steps directly, allocated
  from the dwarf pool by capability. This is the tightest integration: regin must
  honour the **five-field dwarf step schema**, the **handover protocol**
  (step-outcome + deliverables between steps), and the supervisor's
  stdin/stdout/idle-relay contract. Highest value (regin develops software inside
  dvalin's gated workflow) but also the most surface and the most ways to violate
  dvalin's process gates — so it goes last, on stable ground.

### Con / risks

- **Two LLMs in one system.** dvalin's invariant is "the engine calls no LLM."
  Integration must keep regin's LLM strictly *inside the executor/dwarf* (regin's
  process), never pull it into the dvalin engine. All three variants preserve
  this — note it as a hard constraint.
- **Capability honesty.** If regin advertises a `develop` capability (B/C) it must
  actually meet dvalin's gates (tests, one-feature-one-PR, RULE-013 commits).
  Start regin's advertised capabilities **ops-only**; earn `develop` later.
- **Containerisation.** dvalin runs dwarfs in podman with credentials injected and
  `ANTHROPIC_API_KEY` unset. regin-as-dwarf (C) must run under that model or
  declare itself a *host/remote* executor (B) — a routing decision, not a blocker.
- **Protocol bridging.** regin speaks a Unix-socket `Request`/`Response`; dvalin
  speaks its JSON API + executor plugin contract. A thin adapter (a regin
  `dwarf`/`execd` subcommand) bridges the two rather than reworking either core.

## Decision

**Recommended direction (to ratify when this DISC is scheduled):** adopt all
three as **sequenced layers — A → B → C** — keeping regin's LLM on regin's side
of every seam.

1. **A — escalation bridge (first).** regin's `problem` lifecycle gains an
   `escalate` action that submits a dvalin BUG/FEAT via dvalin's JSON API and
   stores the returned ticket id on the problem; regin then verifies on release
   and records a Change. Spawns: regin FEAT (escalation client) + dvalin FEAT
   (accept/ack inter-agent tickets, report status back).
2. **B — ops executor plugin (next).** A regin `execd`/plugin adapter advertises
   ops step-types; dvalin routes matching steps to it. Spawns: regin FEAT
   (executor adapter + step handlers) + dvalin FEAT (register regin as an
   executor, capability routing entry).
3. **C — dwarf-pool membership (last).** regin registers as a capability-scoped
   dwarf honouring the step + handover contract, allocated from the pool. Spawns:
   regin FEAT (dwarf-mode entrypoint + handover) + dvalin FEAT (dwarf identity /
   capacity entry for a host/remote LLM-bearing agent).

**Why this order:** value-per-cost falls and coupling/risk rises from A to C, so
we ship the loop-closing escalation first, make regin a typed engine worker
second, and grant it full agent status only once the contract is proven — exactly
the "self-healing only on stable ground" principle from dvalin's own discovery
methodology.

## Spawned features

To be derived on close (both repos). Sketch:

- regin: `problem escalate` → dvalin JSON API client; release-watch → verify → Change
- dvalin: accept inter-agent tickets from regin; status callback
- regin: executor adapter advertising ops step-types (`monitor.verify`, `ops.remediate`, `incident.triage`)
- dvalin: register regin as an out-of-process executor; capability/platform routing entry
- regin: `dwarf` mode honouring the five-field step schema + handover protocol
- dvalin: dwarf-pool identity + capacity for a host/remote LLM-bearing agent
