---
id: FEAT-072
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-020
depends_on: FEAT-071
---

# FEAT-072 — llm.rs pure extraction + mock-HTTP test

## Description
**As** the project
**I want** `llm.rs` parsing/building logic unit-tested and the real send path
covered
**So that** the LLM client reaches 100% without relying on the live API.

## Implementation
- Extract pure functions and test them: request-body building, **SSE/stream chunk
  parsing**, `msg_to_value`, `tool_result_message`, and tool-call assembly from
  streamed deltas.
- Cover the **real reqwest send path** (`chat_turn` / `chat_completion` /
  `stream_messages`) with a test against a **local mock HTTP server** (dev-dep, e.g.
  `httpmock`) that serves a canned completion and a canned SSE stream — no live API.
- Error paths (HTTP error, malformed SSE) covered.

## Acceptance Criteria
1. Stream/SSE parsing + request building have direct unit tests (incl. malformed
   input).
2. A mock-HTTP test drives the real client send for both non-stream and stream paths,
   asserting parsed output.
3. `llm.rs` line coverage approaches 100% (no live-network dependency).
