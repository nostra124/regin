---
id: FEAT-030
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

# FEAT-030 — Soul configurator + value catalog

## Description
**As** an operator
**I want** to choose regin's values from a catalog of the most prominent values from
human history & literature — and get a sensible starting set **derived from regin's
Persona/role**
**So that** the Soul (FEAT-029) has a coherent identity to vote from on day one.

This is the **seed** stage of DISC-018's value pipeline, and it implements the
**core + per-role overlay** model: a persistent **identity-core** value set (portable,
`identity.db`) plus a swappable **per-Persona overlay** (`persona.toml`).

## Implementation
- **Value catalog (bundled, versioned data — e.g. compiled-in `values.toml`).** A
  curated set of prominent values/virtues, each with `id`, `name`, one-line canonical
  description, and `tradition` tag. Drawn broadly so it is not parochial:
  - **Cardinal / Stoic:** prudence, justice, courage (fortitude), temperance.
  - **Theological:** faith, hope, charity (love).
  - **Confucian:** ren (benevolence), yi (righteousness), li (propriety), zhi
    (wisdom), xin (integrity/trust).
  - **Aristotelian / classical:** honesty, generosity, magnanimity, friendliness.
  - **Chivalric / Bushido:** honour, loyalty, courtesy.
  - **Enlightenment / modern:** liberty, dignity, tolerance, reason, transparency,
    accountability, fairness.
  - **Schwartz basic human values (modern taxonomy backbone):** benevolence,
    universalism, self-direction, achievement, security, conformity.
  - **Agent-operational virtues:** integrity (never fabricate), diligence /
    thoroughness, prudence / caution (do no harm), stewardship (protect entrusted
    systems), humility (escalate when unsure), restraint (don't over-act).
- **Configurator (CLI), `regin soul …`:**
  - `regin soul values list | show <id>` — browse the catalog.
  - `regin soul charter …` — set/edit the **identity-core** charter: a chosen subset
    (+ optional operator-authored custom values and red-lines). Written to
    `identity.db` as `principle`-category memories, `source=human`, pinned. The core
    charter is **not runtime-rewritable by the agent** — only via this human CLI
    (DISC-018 Q5 seed; mirrors DISC-009 red-lines).
  - `regin soul charter show` — render the active grounding (core ∪ active overlay).
- **Persona → values overlay & derivation:**
  - A Persona may declare `values = [ids…]` in `persona.toml` (FEAT-011) — its
    **overlay** (role-specific emphasis layered on the core).
  - `regin soul charter --derive` proposes a starting set for the active Persona from
    a built-in **role → values map** (e.g. `cfo` → prudence, integrity,
    accountability, stewardship; `dev-lead` → diligence, collaboration, courage,
    pragmatism; `operator` → prudence, reliability, stewardship, restraint,
    transparency; `security` → vigilance, integrity, least-privilege, accountability),
    with an LLM-assisted suggestion for novel roles. The proposal is always shown for
    **human confirmation** before it is written (consistent with FEAT-031's
    human-ratified posture).
- **Grounding union:** the active grounding the Soul reads = identity-core values ∪
  active Persona overlay (deduplicated).

## Acceptance Criteria
1. The catalog loads and is versioned; `regin soul values list` shows entries with
   descriptions and tradition tags.
2. `regin soul charter --derive` for a known Persona proposes that role's default
   set; on confirmation it is written as pinned, `source=human`, `principle` seeds in
   `identity.db`.
3. A Persona's explicit `values` overlay is layered onto the core; `regin soul
   charter show` renders the deduplicated union.
4. The core charter cannot be modified except via the CLI — agent/reflection writes
   cannot overwrite seed values (unit-tested).
5. The Soul (FEAT-029) reads exactly this union.
