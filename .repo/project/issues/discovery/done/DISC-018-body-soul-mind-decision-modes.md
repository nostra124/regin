---
id: DISC-018
type: discovery
priority: high
status: done
complexity: L
spawned_features: [FEAT-028, FEAT-029, FEAT-030, FEAT-031, FEAT-032]
---

# DISC-018 — Body / Soul / Mind: dual decision modes and the soul gate

## Identity-plane context

This sits on the **identity plane** alongside DISC-017. Where DISC-017 is *what
regin knows* (the memory plane), this is *how regin decides*. It introduces an
**emotional / identity gate** on top of the logical planner, and a second runtime
**thinking mode**, so that consequential actions are checked against the agent's
true identity before they execute.

## Sharp glossary (load-bearing — used verbatim in the spawned features)

Four non-overlapping terms. Identity is split cleanly: **Persona** is the *outward*
identity (the mask), **Soul** is the *inner* identity (the values). The **Mind** is
not an identity at all — it is the reasoning engine; its tendency to rationalize is
*why* the Soul exists, not a second self.

| Term | One line | In regin |
|---|---|---|
| **Persona** | the *outward* identity — the role regin acts as (the mask) | role id + system-prompt preamble + capability/tool ceiling (FEAT-011) + a per-role **value overlay** (FEAT-030) |
| **Mind** | the *reasoning* — plans and decides | the planning/deciding LLM call; emits a plan + actions |
| **Soul** | the *inner* identity — the values-grounded conscience | a deliberately **starved** LLM vote on the Mind's plan; sees plan intent + values only |
| **Body** | *execution* | tool dispatch — **already built** |

**Where values live (decided 2026-06-19): core + per-role overlay.** The Soul votes
from the agent's persistent **identity-core values** (portable, in `identity.db`,
travels across machines *and* across Persona changes) **plus** the **active Persona's
value overlay** (role-specific emphasis, in `persona.toml`, swappable with the role).
Persona is the *hat*; the Soul's core is the *character*. The hat adds emphasis; it
never resets the character.

The two runtime modes, in this vocabulary:

- **Act** — `Mind → Body`. One pass. Fast. The default.
- **Deliberate** — `Mind ⇄ Soul → Body`. Mind plans read-only, Soul votes, they
  iterate, the approved plan is executed.

## Describe

Today regin runs one way: the **Mind** receives input, decides, and emits tool
calls (`Mind → Body`). That is the right behaviour under pressure and for routine
work — but it leaves the Mind's reasoning unchecked on consequential, novel
decisions, where a clever chain of logic can rationalize the wrong thing. Good
decisions need the **Soul's** approval: a values-grounded *feeling* on whether the
plan is the right thing to do. This is not folklore — it mirrors dual-process
theory (System 1 gut vs System 2 logic) and Damasio's somatic-marker hypothesis
(emotion as the gate on sound decisions).

The two modes in detail:

- **Act** (`Mind → Body`): input → Mind decides → Body executes. One pass. Fast.
  The default.
- **Deliberate** (`Mind ⇄ Soul → Body`):
  1. The Mind plans **read-only** (may read memory + environment, **no side
     effects**) and produces a plan + reasoning.
  2. The **Soul** returns a confidence/feeling vote on the plan. It is deliberately
     **starved**: it sees the plan's *intent / summary* and the values subset only —
     **not** the Mind's full reasoning, the tools, or the environment. That
     starvation is what makes it a *feeling* and not a second round of logic the
     Mind could out-argue.
  3. Mind and Soul iterate until aligned.
  4. The approved plan is handed to the executor to run.

Deliberate mode is therefore, structurally, a **safety gate**: read-only planning
plus a veto before any side-effecting / outward / irreversible action — an
automated form of "confirm before doing something hard to undo."

## How it composes with what exists

- **Engine pieces already exist.** The mind acting = `llm::chat_turn` (with tools);
  the mind planning = a read-only `chat_completion`; the **soul** = a tool-less
  `chat_completion` with a constrained prompt. The body = `tools::execute_tool`.
- **Personas (FEAT-011)** carry the role; the mode attaches per role + per-action
  risk. "Which role regin is working" is a modifier on the trigger.
- **Naming — avoid collisions.** `reflect.rs` already means *memory consolidation*
  and `planning.rs` already means *org cadence planning* (DISC-006). So the modes
  are named **act** / **deliberate** (not "reflection mode"), and the gate is **the
  soul** — keeping body/soul/mind as the conceptual vocabulary.
- **Relationship to DISC-009 (risk guardrail) — two orthogonal gates in sequence.**
  The soul-gate judges **wisdom / alignment** ("is this the right thing to do?");
  DISC-009's three-lane routing judges **authorization** ("may regin auto-apply, or
  must a human approve?"). DISC-018 **reuses DISC-009's blast-radius / reversibility
  judgement** as the trigger for entering deliberate mode. A plan can pass the soul
  yet still need human approval per the guardrail; or be auto-applyable yet be
  soul-vetoed as unwise. Both escalate via the same routing (DISC-010).
- **Relationship to DISC-017 (memory plane).** The soul reads a *values/principles*
  subset of long-term memory; deliberations are captured back into it. This
  **extends the DISC-017 schema**: add `principle` to `memories.category` and
  `deliberation` to `episodes.kind`.

## Variants considered (the soul gate)

| Variant | Summary | Key trade-off |
|---|---|---|
| A | No Soul — Mind only (status quo) | Simplest; the Mind's reasoning is unchecked, no values alignment |
| B | Soul as a *smarter critic* with full context (LLM-as-judge / Reflexion) | Strong logic check; but it merely re-runs the mind's reasoning — not a *feeling*, and can be out-argued |
| C | Soul as a **starved, identity-grounded feeling vote with veto** | A true gut-check the ego can't out-argue; needs mode plumbing + grounding curation |
| D | Hard-coded rule/policy gate | Predictable; can't capture identity nuance, brittle on novel decisions |

## Decision (resolved with user — guided Q&A 2026-06-19)

**Q1 — Trigger: risk-gated (+ role / urgency modifiers).** The primary axis is the
**blast-radius / reversibility of the contemplated action**, reusing DISC-009's
judgement: irreversible / destructive / outward-facing → **deliberate**; read-only
/ reversible / time-critical → **act**. Role (persona) and stress/urgency are
modifiers. This places deliberate mode in the *important-but-not-urgent* quadrant —
consistent with the human rule that high pressure **and** routine work both default
to action.

**Q2 — Deadlock: default-deny + escalate to human.** The soul holds a **veto**.
After the iteration cap without alignment, the action is **denied** and **escalated
to a human** via the escalation bridge (FEAT-015), routed by runtime mode
(DISC-010). Humans break genuine mind/soul ties.

**Q3 — Soul grounding: a values/principles subset (the "true identity").** The Soul
reads a deliberately narrow slice of long-term memory: **pinned + human-authored
facts + the `principle` (values) category + topic summaries** — not arbitrary
facts/trivia. The active grounding is the **identity-core values** (portable, in
`identity.db`) **unioned with the active Persona's value overlay** (per-role, in
`persona.toml`) — core + overlay (decided 2026-06-19). This is the identity it votes
from.

**Q4 — Calibration: capture & learn.** Each deliberation (plan + soul vote +
eventual outcome) is recorded as a new episode kind **`deliberation`** and fed to
the consolidation loop, so the soul's judgement **calibrates against real outcomes**
over time — the identity grows wiser, closing the self-improvement loop with the
memory plane.

**Q5 — Deriving the principles: seed + reflection-proposes + human-ratified.**
Principles are **not** ordinary reflection output — that would let the Mind author
the very values it is checked against (the conscience would not be independent).
Instead, a **three-stage pipeline** with reflection bounded to a *proposing* role:

1. **Seed (constitution).** A small **human/owner-authored charter** of core values +
   red-lines, set once and **not runtime-rewritable by the agent** — same
   constitutional posture as DISC-009's non-adjustable global red-lines. This is the
   floor the soul reads from day one, before any experience exists.
2. **Propose (reflection's role).** The consolidation loop — drawing especially on
   the `deliberation` episodes (plan + vote + **outcome**, Q4) — surfaces principle
   ***candidates*** (e.g. "overriding the gut on irreversible changes repeatedly went
   badly → candidate: don't auto-apply irreversible changes without a backout").
   Candidates are written as candidates, **never** auto-activated.
3. **Promote (human-ratified).** A candidate becomes an **active `principle` only
   after a human ratifies it**, routed via the escalation bridge (FEAT-015 /
   DISC-010). This keeps the conscience independent of the ego: reflection may
   *surface* values from experience, but only a human *enacts* them.

Principles are **sticky**: pinned-like, slow decay; retiring/revising one is gated
(human or overwhelming counter-evidence), not casual churn like facts. Faithful to
the body/soul/mind model — the true identity is partly *given* (charter) and partly
*formed by deep, validated experience* (ratified candidates), never rewritten by
day-to-day logic.

## Design defaults (proposed; refine on the spawned features)

- **Soul output shape:** a confidence scalar (0–1) + a one-line gut reaction +
  verdict `{approve | revise | veto}`. No tool/environment access; sees plan intent
  + identity subset only.
- **Iteration:** configurable `decision.deliberate.max_rounds` (e.g. 3) and
  `decision.deliberate.confidence_threshold` (e.g. 0.7); on hitting `max_rounds`
  without alignment → default-deny + escalate (Q2).
- **Module layout:** a regin-core cognition layer (e.g. `mind.rs` read-only planner
  + `soul.rs` gate), driven by the agent loop; mode selectable per persona + per
  action-risk.
- **Cost:** act mode stays the cheap default; deliberate mode's extra cost
  (mind + soul × rounds) is bounded by the caps above.
- **Read-only planner / executor split:** planning produces **no** side effects; the
  approved plan is handed to a separate executor — a clean safety boundary.

## Open sub-questions (for the spawned features)

1. Exact source of the risk signal — confirm reuse of DISC-009's judgement vs a
   lighter pre-classifier over the planned tool calls.
2. Whether material deviation between the approved plan and the executor's actions
   **re-triggers** the gate.
3. Tuning the soul's confidence threshold / posture (conservative ↔ trusting) —
   possibly **adaptive** like DISC-009 Q2 (earn-trust-with-evidence on KPIs).

## Spawned features (derived — see `feature/open/`)

- **FEAT-028 — Dual-mode agent loop** (act vs deliberate): mode selection from
  action risk (DISC-009) + Persona + urgency; deliberate = read-only Mind plan →
  gate → executor (the planner/executor split lives here).
- **FEAT-029 — The Soul gate**: starved, values-grounded vote (confidence +
  `{approve | revise | veto}`), veto, iteration cap, default-deny + escalate
  (DISC-010/FEAT-015).
- **FEAT-030 — Soul configurator + value catalog**: bundled catalog of prominent
  values from human history/literature; CLI to seed the charter; Persona→values
  defaults (derive a starting set from the role). This is the Q5 *seed* stage.
- **FEAT-031 — Principle derivation & ratification**: `principle` memory category;
  reflection proposes candidates from `deliberation` outcomes; **human-ratified**
  promotion; sticky / slow-decay. The Q5 *propose* + *promote* stages.
- **FEAT-032 — Deliberation capture**: `deliberation` episode kind (plan + vote +
  outcome) wired into consolidation for calibration (Q4).

FEAT-030/031 extend the DISC-017 memory schema (add the `principle` category);
FEAT-032 adds the `deliberation` episode kind — both as additive migrations on the
FEAT-021 store.
