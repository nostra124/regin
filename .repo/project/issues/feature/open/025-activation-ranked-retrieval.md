---
id: FEAT-025
type: feature
priority: high
complexity: L
estimate_tokens: 60k-110k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-017
depends_on: FEAT-021
---

# FEAT-025 — Activation-ranked retrieval

## Description
**As** regin
**I want** memory recall ranked by activation — relevance plus recency, reinforcement,
and trust — and self-reinforcing on use
**So that** the strongest, most-used knowledge surfaces first within a context budget.

Replaces the FEAT-006 `LIKE`-based `memory_search` with FTS5 + an ACT-R/HRR rerank.

## Implementation
- **Candidate selection:** `memories_fts` / `transcripts_fts` BM25 query + optional
  tag / topic / `host` filter (`host IS NULL OR host = :current_host`).
- **Rerank by activation** = f(BM25 rank, recency via `last_retrieved`,
  `retrieval_count`, `trust_score`, `strength`). Tunable weights with sane defaults.
- **Recall reinforcement:** each returned hit bumps `retrieval_count` and
  `last_retrieved` (ACT-R/HRR self-reinforcement), so used memories strengthen.
- **Context injection:** order injected memories by activation; cap to a context
  budget; pinned/high-trust surface first.
- Wire into the chat/agentic system-prompt assembly in place of the old search path.
- Vector recall (FEAT-026) layers onto this additively; this ticket is FTS +
  activation only and must work with `embedding` absent.

## Acceptance Criteria
1. Retrieval returns FTS matches reranked by activation; weighting changes order as
   expected (unit-tested with fixtures).
2. `host`-scoped memories are returned only on the matching host; identity-global
   (`host IS NULL`) always eligible.
3. Each returned hit increments `retrieval_count` and updates `last_retrieved`.
4. Injection respects the context budget and orders by activation (pinned first).
5. Works with no embeddings present; unit-tested.
