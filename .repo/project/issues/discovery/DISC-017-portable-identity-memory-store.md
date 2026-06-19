---
id: DISC-017
type: discovery
priority: high
status: open
complexity: L
spawned_features: ~
---

# DISC-017 — Portable agent identity: the self-improving memory store (its own SQLite DB)

## Identity-plane context

This DISC introduces an axis the operator-plane DISCs (DISC-008..011, DISC-015/016)
do not address. Those concern the **machine apparatus** regin uses to run *a
particular box* via ITIL. This DISC concerns the **agent identity** — the part of
regin that is *itself* and must survive being moved between machines/containers.

The two are strictly separate and must not share a store:

| | **Identity** (this DISC) | **Machine apparatus** (DISC-008..016, existing) |
|---|---|---|
| Portability | copyable container → container; travels with the agent | rebuilt per machine; bound to the box |
| Store | its **own SQLite DB** | the machine-local `regin.db` + files (`desired/`, `filters/`, derived checks) |
| Contents | archived sessions, documented transcripts, topic-based knowledge summaries, distilled semantic memory | ITIL records (incident/change/problem/KPI), audit results, local tools/skills/docs |
| Nature | self-improving; *what the agent has learned* | rule-driven operations of *this* machine |

Two clarifications that motivate this DISC:

- **The audit is apparatus, not identity.** The traceability/self-audit machinery
  (`operations/audit.md`, DISC-016) executes rules and raises ITIL incidents bound
  to the machine's process. It has *nothing* to do with the identity and must not
  be conflated with self-improvement.
- **"Hermes" is an external reference project**
  ([hermes-agent.nousresearch.com](https://hermes-agent.nousresearch.com),
  [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent)) — the
  *inspiration* for this memory design, not a regin subsystem. (DISC-002 / FEAT-006
  used "Hermes" as an internal nickname; that naming is loose and is not rewritten
  here per the audit's "don't retroactively rewrite shipped tickets" rule.)

## Describe

regin's self-improving memory is the substance of its identity: the more it
operates, the more it should know — and that knowledge should follow the agent,
not the machine. Today it does neither cleanly:

1. **Storage is co-located.** DISC-002 → FEAT-005 (episodic tier) + FEAT-006
   (reflective episodic → semantic distillation) put memory in the **same
   `regin.db`** as settings, task runs, ITIL records, and KPIs. Copying the
   identity to a new container would drag the old machine's incidents/changes with
   it, or require surgical extraction. There is no clean seam.

2. **The identity is thinner than it should be.** FEAT-005/006 capture *episodes*
   (run/incident/chat) and distil *semantic memories* (fact/preference/pattern/…).
   They do **not** capture what you describe as the core of the identity:
   - **Archived sessions** — the agent's own past working sessions, retained.
   - **Documented transcripts** — readable records of what was done and decided.
   - **Topic-based knowledge summaries** — durable, subject-organized knowledge
     consolidated from the above (closer to Hermes' organization than to a flat
     `memories` table).

The goal: a dedicated, **portable identity store** (its own SQLite DB) holding the
self-improving memory — sessions, transcripts, topic knowledge, and distilled
semantic memory — with a clean boundary so it can be lifted from one container and
dropped into another, while the machine-local ITIL/audit/operational state stays
behind.

## Variants considered

> First-cut framing only — to refine in guided Q&A. The decision matrix and Pro/Con
> are deliberately left for the Q&A so the options aren't prejudged.

| Variant | Summary | Key trade-off |
|---|---|---|
| A | **Second SQLite DB** (`identity.db`) alongside `regin.db`; move memory tables there, add session/transcript/topic-knowledge tables | Clean portable seam, one extra file, in-stack (rusqlite); needs a migration off `regin.db` |
| B | Keep one DB; mark identity tables and export/import a subset on copy | No second file; but export logic is fiddly and the seam is by-convention, not physical — easy to violate |
| C | Identity as files (markdown sessions/transcripts/topic docs) + a small index DB | Most human-readable / git-friendly; loses transactional consolidation and cross-linking the reflection loop wants |
| D | External store (vector DB / embeddings) for topic knowledge | Powerful recall; reintroduces the heavy dependency DISC-002 already rejected at this scale |

## Open questions (to resolve with user)

1. **Boundary** — is the split exactly *identity = memory* vs *machine = everything
   else (ITIL/audit/KPI/runs/settings)*? Where do **chat conversations** and
   **task-run history** fall — identity (the agent did them) or machine (they
   happened on this box)?
2. **Store shape** — second SQLite DB (Variant A), files+index (C), or a hybrid
   (semantic memory + topic summaries in SQLite; raw transcripts as files)?
3. **Memory organization** — what is the schema for *topic-based knowledge*? How do
   archived sessions → documented transcripts → topic summaries flow (the
   consolidation pipeline), and how does it relate to the existing episodic →
   semantic reflection (FEAT-005/006)? Does this *replace*, *wrap*, or *sit beside*
   that loop?
4. **Portability mechanics** — copy the file as-is? A `regin identity export/import`
   verb? What identifies an identity (so two identities don't collide on one box,
   and one identity can span boxes)? How does identity relate to the dwarf identity
   in DISC-003?
5. **Machine-scoped knowledge** — some learned knowledge is *about a specific
   machine* (this host's quirks). Does that travel with the identity, stay with the
   machine, or get tagged per-host inside the identity store?
6. **Migration** — how do we move FEAT-005/006's existing `episodes` / `memories`
   tables out of `regin.db` without losing data, and is that one FEAT or several?
7. **Naming** — settle the subsystem name now that "Hermes" is reserved for the
   external reference (e.g. "identity store", "memory plane").

## Decision

_Pending — to be discussed with the user (guided Q&A), like DISC-008 / DISC-016._

## Spawned features

_Pending DISC close. Likely: identity-store schema + second DB; migration of
episodic/semantic tables; session-archive + transcript capture; topic-knowledge
consolidation; `identity export/import` portability verb._
