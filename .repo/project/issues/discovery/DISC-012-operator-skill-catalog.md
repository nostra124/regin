---
id: DISC-012
type: discovery
priority: high
status: open
complexity: L
spawned_features: ~
---

# DISC-012 — Operator skill catalog (the Mode-B monitors & remediations)

## Operating-plane context

Operator plane (see DISC-008). Defines the concrete catalog of **monitor +
remediation** skills regin ships to operate a standalone Linux box, mapping 1:1 to
the to-be-state domains (DISC-008). Distinct from DISC-007 (done), which is the
*org/role* skill-package layering for dvalin caves; this is *what regin can actually
watch and fix* on the machine it runs.

## Describe

Today regin ships **three report-only skills** (`disk-usage`, `security-audit`,
`uptime-report`) — LLM prompts that summarize and stop. The converged operator model
(DISC-008 to-be-state, DISC-009 three-lane remediation, DISC-011 ITIL) needs domain
skills that **close the loop**: gather signals, LLM-judge them against the domain's
to-be-state, raise incidents on deviation, and carry **candidate remediations** routed
through the DISC-009 guardrail.

This DISC settles three things: the **anatomy** of an operator skill, the **v1 catalog
scope** (which domains), and how the catalog **ships/extends**.

## Variants considered

| Point | Options | Leaning |
|---|---|---|
| Skill anatomy | (a) keep report-only prompts; (b) **structured bundle**: monitor + default to-be-state domain file + remediation playbook (fixes tagged for DISC-009 lanes) | (b) structured bundle — one skill ↔ one to-be-state domain |
| v1 scope | minimal (disk/services/logs) · **core operational set** · broad (~12 domains) | core operational set |
| Remediation depth | all domains carry remediations · **mixed (remediate only where a safe/reversible fix exists; else monitor-only + escalate)** | mixed, per-domain honesty |
| Packaging | ad-hoc skills · **`regin-operator-skills` system package** (DISC-007 area-style), user-overridable | system package, user-over-system layering |

## Decision matrix

| Criterion | Weight | Report-only (today) | Structured bundle (leaning) |
|---|---|---|---|
| Closes the operator loop (monitor→incident→remediate) | high | ✗ | ✓ |
| Maps cleanly to to-be-state domains (DISC-008) | high | ✗ | ✓ |
| Reuses DISC-009 lanes / DISC-011 ITIL | high | ~ | ✓ |
| Authoring cost | med | ✓ | ~ |

## Open questions (resolving with user)

1. **Anatomy** — confirm the structured bundle (monitor + to-be-state domain file +
   remediation playbook), one skill per domain?
2. **v1 scope** — which domains ship in v1 (core operational set vs minimal vs broad)?
3. **Remediation depth** — mixed (remediate only where a safe/reversible fix exists,
   else monitor-only + escalate), or hold all v1 skills to monitor-only first?
4. **Packaging** — ship as a `regin-operator-skills` system package (the 3 existing
   report-only skills folded in), user-overridable via the existing layering?

## Decision (resolved with user — guided Q&A 2026-06-19)

**Q1 — Anatomy: structured bundle.** Each operator skill ↔ one to-be-state domain,
bundling: a **monitor** (gather signals + LLM-judge against the domain's to-be-state),
the **default to-be-state domain file** (user-editable, DISC-008), and a **remediation
playbook** of candidate fixes, each tagged for a DISC-009 lane. Replaces the
report-only prompt shape.

**Q2 — v1 scope: broad (~12 domains).** disk, systemd services, memory/load, logs,
security-updates, certificates (TLS expiry), backups, network/connectivity, time-sync
(NTP/drift), users/auth, processes, firewall. (Package-integrity is a candidate 13th /
extension.)

**Q3 — Remediation depth: mixed, per-domain.** Remediations ship only where a safe,
reversible fix genuinely exists; the rest are monitor-only + escalate. Indicative
split:
- **Remediating** (safe-lane or `pending_approval`): disk (clear temp, rotate/compress
  logs, clean package cache), services (restart a failed unit), logs (rotate/compress/
  truncate), time-sync (restart chrony/ntp, force sync), backups (trigger a backup
  run), security-updates (apply — mostly `pending_approval`).
- **Monitor-only + escalate** (no safe auto-fix; several touch DISC-009 red-lines):
  memory/load, certificates, network, users/auth, processes, firewall.

**Q4 — Packaging: `regin-operator-skills` system package.** Ships as a versioned
system skill package (DISC-007 area-style / FEAT-014), installed for Mode-B; the three
existing report-only skills (`disk-usage`, `security-audit`, `uptime-report`) fold in /
upgrade into it; fully **user-overridable** via the existing user-over-system layering
and authorable via the FEAT-007 creation flow.

## Spawned features

- **Operator-skill format** — the structured monitor + to-be-state-domain +
  remediation-playbook bundle (each remediation tagged for a DISC-009 lane); the skills
  engine runs the monitor, judges vs to-be-state, raises incidents (DISC-011), and
  offers remediations to the guardrail (DISC-009). Milestone 0.5.0.
- **`regin-operator-skills` package (v1, ~12 domains)** — author the broad domain set
  with default to-be-state files + remediation playbooks (remediating vs monitor-only
  per Q3); fold in the 3 existing report-only skills. Milestone 0.5.0.
- **User override + authoring** — operator skills overridable via user-over-system
  layering; the FEAT-007 skill-creation flow extended to scaffold an operator skill
  (monitor + to-be-state + remediations). Milestone 0.5.0.
