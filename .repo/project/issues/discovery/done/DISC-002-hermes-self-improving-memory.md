---
id: DISC-002
type: discovery
priority: high
status: done
complexity: M
spawned_features: [FEAT-005, FEAT-006]
---

# DISC-002 — Hermes: self-improving, tiered memory

## Describe

regin already has a flat long-term memory store (`memories` table; categories
fact / preference / pattern / project / skill / person), injected as context on
every turn. It does not **improve over time**: memories are only created when a
human runs `regin memory save`. The agent learns nothing from its own task runs,
incidents, or chats.

We want a **self-improving memory** ("Hermes"): regin should reflect on its own
activity and distil durable knowledge automatically, in two tiers:

- **Episodic tier** — short-term, high-volume record of *what happened*: each
  task run, incident touch, and notable chat outcome, with enough context to
  reflect on later. Cheap to write, time-bounded, decays/rolls off.
- **Semantic tier** — the existing long-term `memories` store: distilled,
  durable facts/patterns/preferences. Written by **reflection** over the
  episodic tier, not only by hand.

The improvement loop is **self-reflective**: on a schedule (and/or after each
run), regind reviews recent episodic entries via the LLM and proposes new or
reinforced semantic memories (e.g. "disk-usage skill repeatedly flags /var/log →
recommend logrotate"), with reinforcement counts so repeated observations
strengthen and stale ones decay.

## Variants considered

| Variant | Summary | Key trade-off |
|---|---|---|
| A | Auto-write semantic memories directly from each run (no episodic tier) | Simplest; but noisy, no consolidation, hard to dedupe/reinforce |
| B | Tiered episodic → semantic with scheduled reflection | More moving parts; but consolidation, reinforcement, decay, less noise |
| C | External vector DB / embeddings store | Powerful recall; heavy dependency, overkill for current scale |

## Decision matrix

| Criterion | Weight | A | B | C |
|---|---|---|---|---|
| Improves over time without human input | high | ~ | ✓ | ✓ |
| Low noise / consolidation | high | ✗ | ✓ | ✓ |
| Minimal new dependencies | med | ✓ | ✓ | ✗ |
| Fits existing SQLite + LLM stack | med | ✓ | ✓ | ~ |

## Arguments

### Pro (Variant B — tiered + reflective)

- The user chose "self reflecting and tiered episodic" explicitly.
- Reflection lets the LLM consolidate many noisy episodes into a few durable
  semantic memories, with reinforcement so recurring signals strengthen.
- Reuses the stack we already have (SQLite + the NanoGPT client + the scheduler);
  no new infra.

### Con / risks

- Reflection costs tokens — bound it (cap episodic window per reflection,
  schedule it, make cadence configurable via a `memory.*` setting).
- Quality risk (LLM writes bad memories) — keep reflection *additive* and
  reversible (`memory list/update/delete` already exist), tag auto-memories with
  a source so a human can audit them.

## Decision

**Chosen:** Variant B — **tiered episodic + semantic with scheduled
self-reflection**.

**Why:** It satisfies "improve over time" without a human in the loop, keeps the
semantic store clean via consolidation + reinforcement, and rides the existing
SQLite/LLM/scheduler stack with no new dependency. A vector store (C) is
premature at this scale; direct auto-writes (A) would pollute the store. We
bound token cost with a configurable reflection cadence and keep auto-memories
auditable and reversible.

## Spawned features

- **FEAT-005** — Episodic memory tier (schema + capture of runs/incidents/chats)
- **FEAT-006** — Reflective distillation: episodic → semantic, with reinforcement & decay
