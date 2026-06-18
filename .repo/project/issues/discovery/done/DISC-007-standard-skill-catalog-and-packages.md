---
id: DISC-007
type: discovery
priority: high
status: done
spawned_features: ~
complexity: L
---

# DISC-007 — Standard skill catalog & role skill-packages

> regin-side. Defines the skills an organization needs and how they bundle into
> deployable packages. Deployment + backup + continuity is dvalin DISC-037.

## Describe

Skills bundle into **deployable packages** (`regin-<x>-skills`, shipped like the
existing `/usr/share/regin/skills/` system layer) that **dvalin deploys** to a
cave per the agent's **role profile** (DISC-030/032). Three layers:

- **Baseline** — every regin has it.
- **Area/function** — the standard workflows of a functional area.
- **Role** — what a specific seat needs (a role package depends on the
  area packages it uses).

## Baseline — `regin-base-skills` (every regin)

| Skill | From |
|---|---|
| Operating discipline / ticket hygiene | methodology |
| Per-cave ITIL (incident/change/problem) | MILESTONE-0.2.0 |
| Messaging-bus client (structured + unstructured, channels) | DISC-004/029 |
| Foreman: supervise local CLI workers, collect status, handover | DISC-004 |
| Self-improvement (Hermes episodic→semantic) | DISC-002 |
| Individual planning (aggregate per-repo When/Which, emit upward) | DISC-006 |
| Meeting participate/chair → minutes | DISC-004/031 |
| Per-repo memory/context keyed by repo path | FEAT-008 |
| Tool self-extension within the role's authorization ceiling | DISC-005 |
| Backup/restore awareness | DISC-037 |

## Role packages

| Package | Role | Key skills |
|---|---|---|
| `regin-ceo-skills` | CEO | vision/charter, prioritization, board chair, owner liaison, matrix-conflict arbitration, top approvals |
| `regin-cso-skills` | CSO | initiative portfolio (DISC-036), planning-cycle facilitation (DISC-035), goal/KPI definition+measurement, prioritization, roadmap synthesis |
| `regin-cfo-skills` | CFO | budgeting, cost tracking (workforce compute), financial reporting, spend approval, forecasting, month-end orchestration |
| `regin-cio-skills` | CIO | org ITIL governance (DISC-034), release/security approval, service mgmt, vendor eval |
| `regin-cto-skills` | CTO | architecture direction, tech standards, technical-risk, architecture approval, dev-discipline lead |
| `regin-cao-skills` | CAO | provisioning/onboarding, capacity/pool mgmt, capability grants, self-improvement-health, **backup policy**, **deputy/continuity mgmt**, offboarding |
| `regin-devlead-skills` | Dev lead (per repo) | repo ownership, decompose, worker supervision, PR/merge approval, stand-up, repo ITIL, escalation |
| `regin-marketing-skills` | Marketing lead | campaign mgmt, content planning, market research (→raven), launch approval, performance measurement |
| `regin-support-skills` | Support/feedback lead | feedback→incident/problem, customer research (→raven), satisfaction measurement |

## Area/function packages (shared, deployed where the area runs)

| Package | Area |
|---|---|
| `regin-itil-skills` | org-wide ITIL processes (DISC-034) — incident/problem/change/service-request/CSI |
| `regin-finance-skills` | month-end close, budgeting, reporting |
| `regin-planning-skills` | planning cycle + initiative portfolio (DISC-035/036) |
| `regin-marketing-area-skills` | campaign/content/research workflows |
| `regin-backup-skills` | backup / restore / verify (DISC-037) |
| `regin-audit-skills` | traceability / compliance audit |

## Decisions (agreed with René)

1. Skills ship as **packages** (`regin-<role|area>-skills`) that **dvalin deploys**
   (DISC-037).
2. **Baseline** package on every regin; **role** + **area** packages layered on by
   role profile.
3. A **backup skill package** is always available (DISC-037, CAO-owned).

## Open questions

- Package **granularity/composition** — role packages depend on area packages
  (proposed) vs. flat per-role bundles.
- How a role package declares its dependency on area packages + baseline.

## Spawned features (to derive on close)

- Skill-package structure + manifest (name, deps, skills)
- Author baseline + the role/area packages above
- Package build/release (the `regin-*-skills` artifacts)
