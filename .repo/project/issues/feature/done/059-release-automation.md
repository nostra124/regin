---
id: FEAT-059
type: feature
priority: high
complexity: M
estimate_tokens: 30k-70k
estimate_time: 45-90min
phase: open
status: open
milestone: 0.5.0
depends_on: FEAT-053
---

# FEAT-059 — Release automation (GitHub release with packages)

## Description
**As a** maintainer
**I want** CI to publish a GitHub release with the deb/rpm/apk artifacts on a version
tag
**So that** releases are repeatable and the install script has a stable source.

## Implementation
- A release workflow triggered on a version tag (matching the `Cargo.toml` version):
  - builds the packages via the FEAT-053 `nfpm` recipe (deb + rpm + apk),
  - generates checksums,
  - creates a **GitHub release** for the tag and attaches the packages + checksums.
- Version is derived from `Cargo.toml` (single source of truth); the tag must match.
- The install script (FEAT-056) consumes "latest release" from here.

## Acceptance Criteria
1. Pushing a version tag produces a GitHub release containing deb, rpm, and apk plus
   checksums.
2. The release version matches `Cargo.toml`; a mismatch fails the workflow.
3. The published artifacts install via FEAT-056 / FEAT-054.
