---
id: FEAT-076
type: feature
priority: high
complexity: S
estimate_tokens: 20k-40k
estimate_time: 30-60min
phase: done
status: done
spawned_from: operator-directive-2026-06-20
---

# Build on Mimir — replace nanogpt.* with mimir.*

## Description
**As an** operator of regin
**I want** regin to reach its LLM only through the **Mimir** gateway
**So that** provider keys, model routing, and policy live in one on-prem
gateway (which itself fronts NanoGPT and 24+ other providers), consistent
with Raven and Dvalin.

## What shipped
- `regin-core/src/llm.rs`: `NanoGptClient` → `MimirClient`. Posts to
  Mimir's OpenAI-compatible `/v1/chat/completions` and authenticates as an
  approved consumer via the `X-Client-Cert-Sha256` header (the agent's
  access credential), instead of `Authorization: Bearer`. Field
  `api_key` → `fingerprint`.
- `regin-core/src/config.rs`: settings `nanogpt.{api_key,model,base_url}` →
  `mimir.{fingerprint,model,base_url}` (default base
  `http://127.0.0.1:8700/v1`).
- `regind`: `llm_client()` reads the `mimir.*` settings; errors clearly
  when `mimir.fingerprint` is unset.
- CLI help + README updated (architecture diagram, quick-start, config keys).

## Credential provisioning
The fingerprint is provisioned out of band — by an operator via the Mimir
console, or by **Dvalin** (`dvalin mimir provision --label regin`), which
calls Mimir's `PUT /api/mimir/v1/consumers/{fingerprint}`. Then:
`regin config set mimir.fingerprint "<fingerprint>"`.

## Acceptance
- [x] regin chats only through Mimir's `/v1`, authenticating by fingerprint.
- [x] `mimir.*` settings replace `nanogpt.*`; daemon reads them.
- [x] `cargo build` + `cargo test` green. (PR nostra124/regin#19)

## Related
- Mimir provisioning endpoint: nostra124/mimir FEAT-464.
- Dvalin provisioning: nostra124/dvalin FEAT-143.
