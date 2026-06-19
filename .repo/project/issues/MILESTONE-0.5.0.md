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

| ID       | Title                                                        | Priority |
|----------|--------------------------------------------------------------|----------|
| BUG-002  | README CLI/config docs drifted from the clap surface         | high     |
| BUG-003  | In-code clap help cites retired context.md files (FEAT-008)  | medium   |
| FEAT-019 | Man pages generated from the clap surface (clap_mangen)      | high     |
| FEAT-020 | Regenerable `.deb` packaging (drop the checked-in staging tree) | high  |

### Discovery (Mode-B operator plane — discuss before filing FEATs)

The operator model (autonomous machine operator, ITIL plane) is captured across
DISC-008..011 below; the operator/foreman plane split is the governing framing
(see `.repo/dvalin/notes.md`). DISC-012..014 remain to be opened.

| DISC     | Topic                                                            | Status |
|----------|------------------------------------------------------------------|--------|
| DISC-008 | Machine to-be-state model (explicit md + implicit thresholds)    | filed, open |
| DISC-009 | Operator remediation + three-lane risk guardrail (auto/approve/problem) | filed, decided |
| DISC-010 | Mode-routed escalation + standalone login greeting               | filed, open |
| DISC-011 | ITIL model extensions (blocking, change→problem, approval, hypotheses) | filed, open |
| DISC-012 | Operator skill catalog (which monitors/remediations)             | to open |
| DISC-013 | Per-skill scheduling + self-resilience (watchdog, API backoff)   | to open |
| DISC-014 | Platform-list expansion to `.apk` + `.rpm` (profile §7 is deb-only today) | to open |
| DISC-015 | Adaptive monitoring economics (LLM-judged triage self-optimizing to cheap checks; CSI) | filed, decided |
| DISC-016 | Periodic operator self-audit (recurring CSI review; promoted-check demotion) | filed, open |

## Delivery prerequisites (required before alpha can start)

| Prerequisite | Ticket | Status |
|---|---|---|
| 100% test coverage | (file at planning) | pending |
| Native packages, all platforms + GitHub release | FEAT-020 (deb); DISC-012 (apk/rpm) | in-flight |
| Install script (PIT-tested) | (file at planning) | pending |
| GitHub wiki landing page | (file at planning) | pending |
| Mobile app (if defined by project) | N/A | n/a |

## Suggested delivery order

1. BUG-002 / BUG-003 — stop the bleeding (docs now match the CLI).
2. FEAT-019 — generate man pages from clap so they cannot drift again.
3. FEAT-020 — regenerable deb (consumes the generated man pages + system skills).
4. (after discovery) Mode-B capability FEATs.

## Exit criteria

- README, man pages, and in-code help all match the actual clap surface, with a
  generation step that prevents future drift.
- `.deb` is produced from source by a committed, repeatable recipe; no build/
  staging artifacts remain in git (RULE-011); `Cargo.toml` version is the single
  source of truth and is stamped into the package.
- Mode-B capability scope is decided via DISC-008..012 and its FEATs shipped.
