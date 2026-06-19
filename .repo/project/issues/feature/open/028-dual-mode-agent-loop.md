---
id: FEAT-028
type: feature
priority: high
complexity: L
estimate_tokens: 70k-120k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-018
depends_on: FEAT-011
---

# FEAT-028 — Dual-mode agent loop (act vs deliberate)

## Description
**As** regin
**I want** two decision modes — **act** (one pass) and **deliberate** (read-only
plan → Soul gate → execute) — chosen by the action's risk, my Persona, and urgency
**So that** consequential/irreversible actions get a values check before they run
while routine work stays fast.

Vocabulary (DISC-018): **Persona** = the role/hat (FEAT-011); **Mind** = the
reasoning that plans/decides; **Soul** = the values-grounded gate (FEAT-029);
**Body** = tool execution.

## Implementation
- **Mode selector** in regin-core: classify the contemplated action's risk reusing
  DISC-009's blast-radius/reversibility judgement — irreversible / destructive /
  outward-facing → **deliberate**; read-only / reversible / time-critical → **act**.
  The active Persona and urgency are modifiers (a Persona may raise/lower its default
  mode; high urgency biases toward act). Config: `decision.default_mode`,
  per-Persona override.
- **Deliberate pipeline** (the `Mind ⇄ Soul → Body` path):
  1. **Mind plans read-only** — produces a structured `Plan { intent_summary,
     steps[], intended_tool_calls[] }` with **no side effects** (planning uses a
     read-only LLM turn; side-effecting tools are not dispatched during planning).
  2. Hand the `Plan` to the **Soul gate** (FEAT-029) → verdict.
  3. On `revise`: feed the Soul's one-line gut reaction back to the Mind, re-plan,
     up to `decision.deliberate.max_rounds` (default 3).
  4. On `approve`: hand the `Plan` to the **executor**, which performs the intended
     tool calls (the Body), each still subject to the Persona ceiling + DISC-009
     lanes.
  5. On `veto`, or `max_rounds` reached without approval: **default-deny + escalate**
     (FEAT-029 / FEAT-015).
- **Planner / executor split:** planning emits a `Plan` value; a separate executor
  performs side effects. The planning path can never mutate — a clean safety
  boundary.
- **Act mode** is today's `chat_turn` path, unchanged, and remains the default.

## Acceptance Criteria
1. A high-risk (irreversible/outward) action enters deliberate mode; a low-risk /
   read-only one stays in act mode — unit-tested with a fake risk classifier.
2. In deliberate mode the Mind's planning phase performs **no** side effects: a
   planning run with a spy executor records zero tool executions.
3. An approved `Plan` is executed exactly once via the executor; a vetoed or
   max-rounds `Plan` is not executed and raises an escalation.
4. `max_rounds` and per-Persona mode override are honoured.
5. Act-mode behaviour is unchanged (regression test).
