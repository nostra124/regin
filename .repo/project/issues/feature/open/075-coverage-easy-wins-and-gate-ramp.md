---
id: FEAT-075
type: feature
priority: high
complexity: S
estimate_tokens: 30k-60k
estimate_time: 45-90min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-020
depends_on: FEAT-074
---

# FEAT-075 — Easy-win unit tests + coverage gate ramp to 100%

## Description
**As** the project
**I want** the trivial untested files covered and the gate raised to 100%
**So that** the 0.5.0 100%-coverage exit criterion is actually enforced.

## Implementation
- Unit-test the currently-untested pure files: `config.rs` (path/settings helpers,
  `regind_service_unit`), `context.rs` (`build_system_prompt` etc.), `types.rs`
  (`ChatMessage` constructors) — and any remaining stragglers surfaced by a baseline
  `cargo llvm-cov` report.
- **Ramp the gate** in the Makefile/CI: raise `COVERAGE_MIN` 55 → 80 → 95 → **100**
  as FEAT-070..074 land, ending at `--fail-under-lines 100` with **no exclusions**.
- Add **per-crate floors** (e.g. `--fail-under-lines 100` evaluated per package, or
  separate jobs) so a binary can't hide behind the library's coverage.

## Acceptance Criteria
1. `config.rs`, `context.rs`, `types.rs` (and any baseline stragglers) are covered.
2. CI enforces `--fail-under-lines 100` workspace-wide with per-crate floors and no
   coverage exclusions.
3. A regression that drops any crate below 100% fails CI.
