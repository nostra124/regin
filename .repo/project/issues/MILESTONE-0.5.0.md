---
milestone: 0.5.0
title: Standalone operator hardening — docs, man pages, packaging, operator skills
status: active
depends_on: 0.4.0
---

# Milestone 0.5.0 — Standalone operator hardening

regin has two operating modes: **(A) dvalin foreman** (built out across 0.3.0/
0.4.0 — bus, foreman, persona, meeting, planning, deputy) and **(B) an
independent 24/7 operator of a Linux/unix environment** (the original scheduled-
skill runner + ITIL + Hermes memory + tool-using chat). Mode B works but is
thin and its docs have drifted from the shipped CLI. This milestone hardens
Mode B: it first removes the accumulated documentation/packaging drift, then
(pending discovery) builds out the operator's skill catalog, remediation
guardrails, notification egress, and scheduling.

The doc/man/packaging items below are filed now (uncontroversial corrections).
The Mode-B capability items are **pending discovery** — they expand behaviour and,
in the case of apk/rpm, the supported-platform list (profile §7), so each needs a
DISC + user decision before a FEAT is minted.

## Issues

### Docs / packaging (uncontroversial)

| ID       | Title                                                        | Priority |
|----------|--------------------------------------------------------------|----------|
| BUG-002  | README CLI/config docs drifted from the clap surface         | high     |
| BUG-003  | In-code clap help cites retired context.md files (FEAT-008)  | medium   |
| FEAT-019 | Man pages generated from the clap surface (clap_mangen)      | high     |
| FEAT-020 | Regenerable `.deb` packaging — **superseded by FEAT-053**    | high     |

### Operator capability (derived from DISC-008..016)

| ID | Title | From |
|----|-------|------|
| FEAT-033 | Desired-state (to-be) format + loader | DISC-008 |
| FEAT-034 | Observed-vs-target monitoring evaluation | DISC-008 |
| FEAT-035 | ITIL schema extensions | DISC-011 |
| FEAT-036 | Recurrence-to-problem rule | DISC-011 |
| FEAT-037 | Three-lane remediation engine | DISC-009 |
| FEAT-038 | Capability ceiling + global red-lines | DISC-009 |
| FEAT-039 | Safe-lane gate (backout + dry-run + blast-radius) | DISC-009 |
| FEAT-040 | Adaptive autonomy posture | DISC-009 |
| FEAT-041 | Effective-mode detection (org vs standalone) | DISC-010 |
| FEAT-042 | Decision/approval escalation (org, over bus) | DISC-010 |
| FEAT-043 | Standalone parking + login greeting | DISC-008/010 |
| FEAT-044 | Critical-only active push | DISC-010 |
| FEAT-045 | Operator-skill format + authoring | DISC-012 |
| FEAT-046 | `regin-operator-skills` package (~12 domains) | DISC-012 |
| FEAT-047 | Per-skill scheduling + jitter | DISC-013 |
| FEAT-048 | Operator resilience (degradation/recovery/watchdog) | DISC-013 |
| FEAT-049 | Two-tier evaluation engine | DISC-015 |
| FEAT-050 | KPI store + `regin metrics` | DISC-015 |
| FEAT-051 | Promotion + demotion loop (derived-checks store) | DISC-015 |
| FEAT-052 | Notice filters | DISC-015 |
| FEAT-053 | nfpm multi-format packaging (deb+rpm+apk) | DISC-014 |
| FEAT-054 | Per-format install PITs | DISC-014 |
| FEAT-055 | Scheduled operator self-audit (CSI review) | DISC-016 |

### Discovery (Mode-B operator plane — discuss before filing FEATs)

The operator model (autonomous machine operator, ITIL plane) is captured across
DISC-008..011 below; the operator/foreman plane split is the governing framing
(see `.repo/dvalin/notes.md`). All operator-plane discoveries (DISC-008..016) are
now filed and decided.

| DISC     | Topic                                                            | Status |
|----------|------------------------------------------------------------------|--------|
| DISC-008 | Machine to-be-state model (explicit md + implicit thresholds)    | filed, decided |
| DISC-009 | Operator remediation + three-lane risk guardrail (auto/approve/problem) | filed, decided |
| DISC-010 | Mode-routed escalation + standalone login greeting               | filed, decided |
| DISC-011 | ITIL model extensions (blocking, change→problem, approval, hypotheses) | filed, decided |
| DISC-012 | Operator skill catalog (which monitors/remediations)             | filed, decided |
| DISC-013 | Per-skill scheduling + self-resilience (watchdog, API backoff)   | filed, decided |
| DISC-014 | Platform-list expansion to `.apk` + `.rpm` (profile §7 is deb-only today) | filed, decided |
| DISC-015 | Adaptive monitoring economics (LLM-judged triage self-optimizing to cheap checks; CSI) | filed, decided |
| DISC-016 | Periodic operator self-audit (recurring CSI review; promoted-check demotion) | filed, decided |

## Delivery prerequisites (required before alpha can start)

| Prerequisite | Ticket | Status |
|---|---|---|
| 100% test coverage | (file at planning) | pending |
| Native packages, all platforms + GitHub release | FEAT-020 → nfpm deb+rpm+apk (DISC-014) | in-flight |
| Install script (PIT-tested) | (file at planning) | pending |
| GitHub wiki landing page | (file at planning) | pending |
| Mobile app (if defined by project) | N/A | n/a |

## Suggested delivery order

**Docs/packaging first:**
1. BUG-002 / BUG-003 — stop the bleeding (docs now match the CLI).
2. FEAT-019 — generate man pages from clap so they cannot drift again.

**Operator capability (dependency order):**
3. **Foundation** — FEAT-035 (ITIL schema) · FEAT-033 (to-be-state format/loader) ·
   FEAT-050 (KPI store) · FEAT-038 (ceiling + red-lines).
4. **Evaluation** — FEAT-034 (observed-vs-target) · FEAT-049 (two-tier engine) ·
   FEAT-036 (recurrence rule) · FEAT-052 (notice filters).
5. **Skills** — FEAT-045 (operator-skill format) · FEAT-046 (`regin-operator-skills`
   package) · FEAT-047 (per-skill scheduling).
6. **Remediation loop** — FEAT-037 (three-lane engine) · FEAT-039 (safe-lane gate) ·
   FEAT-041 (effective-mode) · FEAT-042/043/044 (escalation, greeting, push).
7. **Optimisation & resilience** — FEAT-051 (promotion/demotion) · FEAT-040 (adaptive
   posture) · FEAT-048 (resilience) · FEAT-055 (self-audit).
8. **Packaging** — FEAT-053 (nfpm deb+rpm+apk) · FEAT-054 (per-format install PITs).

## Exit criteria

- README, man pages, and in-code help all match the actual clap surface, with a
  generation step that prevents future drift.
- Packages (deb + rpm + apk) are produced from source by a single committed `nfpm`
  recipe (FEAT-053); no build/staging artifacts remain in git (RULE-011);
  `Cargo.toml` version is the single source of truth; per-format install PITs pass.
- The operator loop is closed: observed-vs-target evaluation → incident (DISC-011
  schema) → three-lane remediation (auto-apply within the safe-lane gate + ceiling/
  red-lines; `pending_approval` routed by effective mode; problem + escalate), with
  the login greeting / critical push surfacing what needs a human.
- The CSI economics run: two-tier evaluation, KPI store + `regin metrics`, promotion/
  demotion of derived checks, notice filters, and the periodic self-audit.
- The `regin-operator-skills` catalog (~12 domains) ships and is user-overridable.
- 100% test coverage; no open design questions remain in any 0.5.0 FEAT (RULE-005).
