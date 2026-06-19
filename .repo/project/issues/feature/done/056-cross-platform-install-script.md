---
id: FEAT-056
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
depends_on: FEAT-053
---

# FEAT-056 — Cross-platform install script (PIT-tested)

## Description
**As a** new user
**I want** a one-command install
**So that** I can get regin running on any supported distro without manual packaging
steps.

## Implementation
- A POSIX `sh` install script (`curl … | sh`) that:
  - detects the distro / package format (deb / rpm / apk),
  - fetches the matching package from the latest GitHub release (FEAT-059),
  - installs it and enables the per-user lingering service,
  - is idempotent (safe to re-run; upgrades in place).
- Clear failure messages for unsupported platforms; no-ops cleanly if already current.
- **PIT-tested** (RULE-003/004) on Debian/Ubuntu, Fedora/RHEL, and Alpine via podman.

## Acceptance Criteria
1. The script installs regin and brings up the daemon on each supported family in a
   podman PIT.
2. Re-running is idempotent (no duplicate install; in-place upgrade).
3. An unsupported platform fails with a clear message; tested.
