---
id: DISC-018
type: discovery
priority: high
status: open
complexity: L
spawned_features: ~
---

# DISC-018 — Body / Soul / Mind: dual decision modes and the soul gate

## Identity-plane context

This sits on the **identity plane** alongside DISC-017. Where DISC-017 is *what
regin knows* (the memory plane), this is *how regin decides*. It introduces an
**emotional / identity gate** on top of the logical planner, and a second runtime
**thinking mode**, so that consequential actions are checked against the agent's
true identity before they execute.

The framing is body / soul / mind:

| Concept | Human | regin |
|---|---|---|
| **Body** | action | tool dispatch + answer + memory read/write — **already built** |
| **Mind** | logic, planning, decision; *home of the ego / "false" identity* | the main LLM context — decides and derives actions |
| **Soul** | emotion, gut, the *true / inner identity* | a second, deliberately **starved** LLM call that returns a **feeling**, grounded only in long-term identity memory |

## Describe

Today regin runs one way: the **mind** receives input, decides, and emits tool
calls (`mind → body`). That is the right behaviour under pressure and for routine
work — but it leaves the clever-but-rationalizing mind (the ego) unchecked on
consequential, novel decisions. Good decisions, in the body/soul/mind model, need
the **soul's** approval: a non-logical, identity-aligned *feeling* on whether the
plan is the right thing to do. This is not folklore — it mirrors dual-process
theory (System 1 gut vs System 2 logic) and Damasio's somatic-marker hypothesis
(emotion as the gate on sound decisions).

Two runtime modes:

- **Act mode** (`mind → body`): input → mind decides → body executes tool calls +
  answers. One pass. Fast. The default.
- **Deliberate mode** (`mind ⇄ soul → body`):
  1. The mind plans **read-only** (may read memory + environment, **no side
     effects**) and produces a plan + reasoning.
  2. A second, constrained call — the **soul** — returns a confidence/feeling vote
     on the plan. It is deliberately **starved**: it sees the plan's *intent /
     summary* and the identity-memory subset only — **not** the mind's full
     reasoning, the tools, or the environment. That starvation is the mechanism
     that makes it a *feeling* and not a second round of logic the ego could
     out-argue.
  3. Mind and soul iterate until aligned.
  4. The approved plan is handed to the action executor to run.

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
| A | No soul — mind only (status quo) | Simplest; the ego is unchecked, no identity alignment |
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

**Q3 — Soul grounding: a values/principles subset (the "true identity").** The soul
reads a deliberately narrow slice of long-term memory: **pinned + human-authored
facts + a new `principle` (values) category + topic summaries** — not arbitrary
facts/trivia. This is the identity it votes from.

**Q4 — Calibration: capture & learn.** Each deliberation (plan + soul vote +
eventual outcome) is recorded as a new episode kind **`deliberation`** and fed to
the consolidation loop, so the soul's judgement **calibrates against real outcomes**
over time — the identity grows wiser, closing the self-improvement loop with the
memory plane.

**Q5 — Deriving the principles: seed + reflection-proposes + human-ratified.**
Principles are **not** ordinary reflection output — that would let the mind (ego)
author the very values it is checked against (the conscience would not be
independent). Instead, a **three-stage pipeline** with reflection bounded to a
*proposing* role:

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

## Spawned features (to derive on close)

- **Dual-mode agent loop** — act vs deliberate; mode selection from action risk
  (DISC-009) + persona/role + urgency.
- **The soul gate** — constrained, identity-grounded vote (confidence + verdict),
  veto, iteration cap, default-deny + escalate (DISC-010/FEAT-015).
- **Values/principles grounding & derivation** — `principle` memory category +
  human-authored seed charter; reflection proposes principle *candidates* from
  `deliberation` outcomes; **human-ratified promotion** to active principle
  (FEAT-015/DISC-010); sticky / slow-decay; extends DISC-017.
- **Deliberation capture** — `deliberation` episode kind (plan + vote + outcome)
  wired into consolidation for calibration (extends DISC-017).
- **Read-only planner / executor split** — planning has no side effects; the
  approved plan is executed separately.
