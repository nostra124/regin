---
id: DISC-020
type: discovery
priority: high
status: decided
complexity: L
spawned_features: FEAT-070..075
---

# DISC-020 — Path to 100% test coverage (no exclusions)

## Context

MILESTONE-0.5.0's exit criterion is **100% test coverage**; it shipped at the
operationalized gate (`cargo llvm-cov --workspace --fail-under-lines`,
`COVERAGE_MIN=55`, currently ~59%). The core library is well covered; the gaps
cluster in three places:

- **`regin-cli/src/main.rs`** (~2069 lines, ~44 `cmd_*` functions) — essentially
  untested: each command does an `rpc()` round-trip to the daemon over a Unix
  socket and renders the `Response`. The single biggest gap.
- **`regind/src/main.rs`** (~998 lines) — `main()`, `accept_loop`,
  `handle_connection`, the scheduler/reflection loops, the LLM-dependent `dispatch`
  arms (chat / task exec), and signal/shutdown. `dispatch` was made generic over
  the writer and partially unit-tested; the rest is uncovered.
- **`regin-core/src/llm.rs`** — the LLM client (reqwest send + SSE/stream parsing),
  plus easy untested pure files `config.rs`, `context.rs`, `types.rs`.

## Decision (resolved with user 2026-06-19)

- **Target = absolute 100%, no exclusions.** Even the irreducible glue (process
  entrypoint, signal handling, socket accept loop, the real `rpc()` transport, the
  real HTTP send) is covered — by **integration tests that spawn the real
  instrumented binaries**, not by `#[coverage(off)]` / ignore config.
- **Home = MILESTONE-0.6.0.** Folded into the identity-plane milestone rather than a
  separate 0.5.1. The testability seams below (injectable LLM, CLI transport seam)
  also make 0.6.0/0.7.0 far easier to test, so they pay forward.

## Strategy

Two pillars: **(A) testability seams** so logic is unit-testable, and **(B)
integration tests that exercise the real binaries** so the glue is covered too.

- **CLI transport seam (A).** Introduce a `Transport` trait (real = Unix-socket
  `rpc`; fake = canned `Response`s). `cmd_*` take a transport and split into pure
  render/logic; unit tests drive every command with canned responses. `clap`
  surface covered by `Cli::try_parse_from` tests.
- **Injectable LLM client (A).** An `LlmClient` trait; `AppState` holds a
  `dyn LlmClient` (prod = `NanoGptClient`, tests = `FakeLlm` returning canned
  turns/streams). Unlocks the daemon's chat/task `dispatch` arms.
- **llm.rs pure extraction (A).** Request-body building, SSE/stream parsing,
  `msg_to_value`, `tool_result_message` become tested pure fns; the **real reqwest
  send** is covered by a test against a **local mock HTTP server** (dev-dep, e.g.
  `httpmock`).
- **Daemon loop extraction (A).** Scheduler/reflection loop *bodies* become testable
  tick functions; the outer `loop {}` is thin.
- **Integration tests, real binaries (B).** Tests in `tests/` spawn
  `regind` (`CARGO_BIN_EXE_regind`) on a temp socket and run `regin`
  (`CARGO_BIN_EXE_regin`) against it, then signal shutdown — covering `main()`,
  `accept_loop`, `handle_connection`, the real `rpc()` transport, and signal paths.
  **cargo-llvm-cov propagates `LLVM_PROFILE_FILE` to child processes**, so the
  spawned instrumented binaries contribute to coverage — this is what makes
  no-exclusion 100% achievable.
- **Gate ramp.** Raise `COVERAGE_MIN` 55 → 80 → 95 → **100** as features land; add
  **per-crate** floors so a binary can't hide behind the library's coverage.

## Spawned features (MILESTONE-0.6.0)

- **FEAT-070 — CLI transport seam + render/logic split**: `Transport` trait + fake;
  pure render fns; `clap` parse tests. Covers `regin-cli` logic.
- **FEAT-071 — Injectable LLM client**: `LlmClient` trait in `AppState`; `FakeLlm`;
  cover the daemon's LLM-dependent dispatch arms.
- **FEAT-072 — llm.rs pure extraction + mock-HTTP test**: tested request/parse fns +
  a local-mock-server test for the real send path.
- **FEAT-073 — Daemon loop extraction + full dispatch coverage**: scheduler/
  reflection tick fns; remaining dispatch arms (persona/bus/meeting/plan/foreman/
  deputy/skill-pkg) via fakes.
- **FEAT-074 — Integration tests (real binaries)**: spawn regind + drive regin over a
  temp socket incl. shutdown; covers entrypoints, accept loop, rpc transport, signals.
- **FEAT-075 — Easy-win unit tests + gate ramp**: `config.rs`/`context.rs`/`types.rs`
  tests; raise `COVERAGE_MIN` to 100 with per-crate floors; Makefile/CI update.
