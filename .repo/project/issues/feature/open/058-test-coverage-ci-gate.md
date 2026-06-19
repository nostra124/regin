---
id: FEAT-058
type: feature
priority: high
complexity: M
estimate_tokens: 30k-70k
estimate_time: 45-90min
phase: open
status: open
milestone: 0.5.0
---

# FEAT-058 — Test-coverage measurement + CI gate

## Description
**As** the project
**I want** coverage measured and enforced in CI
**So that** the milestone's 100%-coverage delivery prerequisite is actually gated, not
aspirational.

## Implementation
- Wire a coverage tool into CI (e.g. `cargo-llvm-cov`) producing a coverage report for
  the workspace (`regin-core`, `regind`, `regin-cli`).
- **CI gate:** fail the build when coverage drops below the project threshold (the
  milestone target is 100%); surface the report as a CI artifact / summary.
- Document the local command to reproduce coverage.
- Exclude only justified, annotated lines (e.g. `unreachable!`), recorded explicitly so
  the gate stays honest.

## Acceptance Criteria
1. CI computes workspace coverage on every PR and publishes the report.
2. The build fails when coverage is below the configured threshold; passes at/above it.
3. The local reproduction command is documented; any exclusions are explicit and
   annotated.
