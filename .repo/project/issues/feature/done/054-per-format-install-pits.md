---
id: FEAT-054
type: feature
priority: medium
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-014
depends_on: FEAT-053
---

# FEAT-054 — Per-format install PITs

## Description
**As** the project
**I want** package-install tests for each format
**So that** deb/rpm/apk installs are verified on their real distros (RULE-004).

## Implementation
- Podman-based PIT (RULE-003/004) per format:
  - `.deb` on Debian/Ubuntu,
  - `.rpm` on Fedora/RHEL,
  - `.apk` on Alpine.
- Each PIT installs the package built by FEAT-053, starts the daemon, and runs a
  smoke check (CLI talks to the daemon; a skill runs; man page present).
- Wired into CI.

## Acceptance Criteria
1. Each format installs cleanly in its distro container and the smoke check passes.
2. A broken package fails its PIT (negative check).
3. The PITs run in CI via podman.
