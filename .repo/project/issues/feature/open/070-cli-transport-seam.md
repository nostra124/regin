---
id: FEAT-070
type: feature
priority: high
complexity: L
estimate_tokens: 80k-130k
estimate_time: 120-180min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-020
---

# FEAT-070 — CLI transport seam + render/logic split

## Description
**As** the project
**I want** the CLI's command logic testable without a running daemon
**So that** `regin-cli` (~2069 lines, the biggest coverage gap) can be unit-tested.

## Implementation
- Introduce a `Transport` trait: `async fn request(&self, Request) -> Result<Response>`.
  Production impl = the existing Unix-socket `rpc()`; a `FakeTransport` returns
  canned `Response`s (and records sent `Request`s) for tests.
- Refactor the ~44 `cmd_*` functions to take a `&dyn Transport` (or a generic),
  splitting each into a **pure render/format** step (`Response -> String`) and the
  transport call. Render fns are directly unit-tested; logic (which request, error
  handling, exit conditions) is tested via `FakeTransport`.
- Add `clap` surface tests with `Cli::try_parse_from` for representative argv across
  the command tree (parsing + arg wiring).

## Acceptance Criteria
1. Every `cmd_*` is exercised via `FakeTransport` (happy path + an error `Response`),
   and each render fn has a unit test on its output.
2. `Cli::try_parse_from` tests cover the command/subcommand/flag surface.
3. The real socket `rpc()` impl is isolated behind `Transport` (covered by FEAT-074);
   `regin-cli` line coverage approaches 100% under unit tests.
