---
id: FEAT-083
type: feature
priority: medium
complexity: L
estimate_tokens: 60k-120k
estimate_time: 1-2h
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-083 — Multi-provider model abstraction

## Description

**As a** regin user
**I want** to use any OpenAI-compatible LLM provider (not just NanoGPT)
**So that** I can choose the best model for the task, switch providers without
rebuilding, and use local models

Currently regin bakes in the NanoGPT HTTP API format and hardcodes the base URL.
This locks users into a single provider. A trait-based `LlmClient` abstraction
lets users configure any OpenAI-compatible endpoint, and paves the way for
non-OpenAI-compatible providers (Anthropic, Google) later.

## Acceptance Criteria

1. Extract `LlmClient` trait in regin-core:
   ```rust
   #[async_trait]
   pub trait LlmClient: Send + Sync {
       async fn chat_stream(...) -> Result<ChatResponse>;
   }
   ```

2. Existing NanoGPT logic becomes `NanogptClient: LlmClient` — no behaviour
   change for existing users.

3. New `OpenaiClient: LlmClient` supports any OpenAI-compatible API by
   configuring:
   - `regin config set llm.base_url <url>` (replaces `nanogpt.base_url`)
   - `regin config set llm.api_key <key>` (replaces `nanogpt.api_key`)
   - `regin config set llm.model <model>` (replaces `nanogpt.model`)

4. Backward compatibility: existing `nanogpt.*` config keys continue to work
   (mapped to `llm.*` internally) with a deprecation warning on first use.

5. The `LlmClient` trait is the single seam for model-provider integration.
   Future providers (Anthropic, Google, Ollama) are additive — they implement
   the trait without touching the agent loop.

6. Unit tests cover: `NanogptClient` streaming, `OpenaiClient` streaming with a
   mock HTTP server, trait dispatch.

7. Integration test: `regin config set llm.base_url "http://localhost:8080/v1"`
   + `regin chat` connects to a local mock endpoint.
