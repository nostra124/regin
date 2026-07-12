---
id: FEAT-071
type: feature
priority: high
complexity: M
estimate_tokens: 60k-100k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-020
---

# FEAT-071 — Injectable LLM client (`LlmClient` trait)

## Description
**As** the project
**I want** the LLM client behind a trait the daemon depends on
**So that** the LLM-dependent dispatch arms (chat, task exec) are testable with a
fake, and the identity/intent planes can inject fakes too.

## Implementation
- Define an `LlmClient` trait covering what callers use (`chat_completion`,
  `chat_turn`, streaming). `NanoGptClient` implements it (production).
- `AppState` holds a `dyn LlmClient` (boxed/`Arc`) instead of constructing
  `NanoGptClient` inline; `AppState::llm_client()` returns the injected client.
- A `FakeLlm` test double returns canned completions / tool-call turns / streamed
  chunks, with no network.
- Test constructor for `AppState` that takes an injected `LlmClient` + in-memory DB.

## Acceptance Criteria
1. Production behaviour is unchanged (NanoGptClient injected by default).
2. Tests construct `AppState` with `FakeLlm` + in-memory DB and exercise a chat /
   task-exec dispatch arm end-to-end without network.
3. The trait is the single seam through which the daemon obtains completions; unit-
   tested.
