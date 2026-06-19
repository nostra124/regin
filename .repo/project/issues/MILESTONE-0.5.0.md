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

### Pending discovery (Mode-B operator — discuss before filing FEATs)

| DISC (proposed) | Topic                                                            |
|-----------------|-----------------------------------------------------------------|
| DISC-008        | Mode-B operator skill catalog (which skills; report vs. remediate) |
| DISC-009        | Autonomy/guardrail model for standalone remediation (allowlist/approval/dry-run + ITIL change) |
| DISC-010        | Notification egress for standalone mode (email/webhook/ntfy/matrix) |
| DISC-011        | Per-skill scheduling + self-resilience (watchdog, API backoff)   |
| DISC-012        | Platform-list expansion to `.apk` + `.rpm` (profile §7 is deb-only today) |

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
