---
id: FEAT-046
type: feature
priority: high
complexity: L
estimate_tokens: 90k-140k
estimate_time: 150-210min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-012
depends_on: FEAT-045
---

# FEAT-046 — `regin-operator-skills` package (v1, ~12 domains)

## Description
**As** an operator
**I want** a ready catalog of operator skills for a standalone box
**So that** regin can monitor and remediate the common domains out of the box.

## Implementation
- A versioned **`regin-operator-skills` system package** (DISC-007 area-style /
  FEAT-014), installed for Mode-B, shipping ~12 operator skills (FEAT-045 format) with
  default to-be-state files:
  disk, systemd services, memory/load, logs, security-updates, certificates (TLS
  expiry), backups, network/connectivity, time-sync (NTP/drift), users/auth,
  processes, firewall.
- **Mixed remediation depth** (per DISC-012 Q3):
  - *Remediating* (safe-lane / approval): disk (clear temp, rotate/compress logs,
    clean pkg cache), services (restart failed unit), logs (rotate/compress/truncate),
    time-sync (restart chrony/ntp, force sync), backups (trigger a backup run),
    security-updates (apply — mostly `pending_approval`).
  - *Monitor-only + escalate*: memory/load, certificates, network, users/auth,
    processes, firewall.
- **Fold in** the three existing report-only skills (`disk-usage`, `security-audit`,
  `uptime-report`) — upgrade them into the structured format.
- Fully user-overridable via user-over-system layering.

## Acceptance Criteria
1. Installing the package provides the ~12 operator skills with default to-be-state
   files; each loads via FEAT-045.
2. Remediating domains offer their playbook fixes through the lanes; monitor-only
   domains raise incidents + escalate without auto-fixing.
3. The 3 legacy skills are folded in (no duplicate/parallel skill); a user override
   shadows a packaged skill; install is PIT-tested.
