---
id: FEAT-074
type: feature
priority: high
complexity: M
estimate_tokens: 60k-100k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-020
depends_on: FEAT-070
---

# FEAT-074 — Integration tests over the real binaries

## Description
**As** the project
**I want** the irreducible binary glue covered by running the real binaries
**So that** absolute 100% (no exclusions) is reachable — `main()`, the accept loop,
`handle_connection`, the real `rpc()` transport, and signal/shutdown.

## Implementation
- A `tests/` integration suite that:
  - spawns `regind` (`CARGO_BIN_EXE_regind`) pointed at a **temp socket + temp data
    dir** (override via env/`XDG_*`), waits for readiness (`regin ping`),
  - runs `regin` (`CARGO_BIN_EXE_regin`) commands against it over the real socket
    (covers CLI `main()` dispatch + the real `rpc()` transport),
  - exercises a representative command set + a bad-request line,
  - then sends **SIGTERM** and asserts clean shutdown (covers the signal/cleanup
    path).
- Rely on **cargo-llvm-cov child-process capture** (`LLVM_PROFILE_FILE` propagation)
  so the spawned instrumented binaries contribute coverage — **no `#[coverage(off)]`
  / ignore exclusions**.

## Acceptance Criteria
1. The suite starts the real daemon on an isolated socket/data dir and drives real
   CLI commands end-to-end, including a bad request and SIGTERM shutdown.
2. Run under `cargo llvm-cov`, the spawned binaries' `main`/accept/handle/rpc/signal
   lines register as covered.
3. The tests are hermetic (no shared global state, no live network) and run in CI.
