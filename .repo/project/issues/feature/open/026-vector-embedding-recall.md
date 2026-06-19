---
id: FEAT-026
type: feature
priority: medium
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-017
depends_on: FEAT-025
---

# FEAT-026 — Vector/embedding recall

## Description
**As** regin
**I want** semantic recall via embeddings layered onto FTS
**So that** topic-based recall finds relevant knowledge even when keywords don't match.

DISC-017 consciously pulls embeddings into the initial memory-plane build — a scoped
override of DISC-002's deferral, **for the memory plane only**. It remains **additive**
to FTS: the store works fully if embeddings are absent.

## Implementation
- **Embedding pipeline:** on consolidation/insert, compute an embedding for each
  `memories` row and store it in the `embedding` BLOB (FEAT-021 column). Backfill
  existing rows lazily/batched. Model + dimensions configurable; failures leave
  `embedding=NULL` and log (no hard dependency).
- **Vector query:** cosine similarity over `embedding` produces a second candidate
  set, **merged with** the FTS/activation candidates (FEAT-025) — hybrid recall, not
  a replacement. Reuse the activation rerank over the merged set.
- **Config:** `memory.embeddings.enabled` (default on), model, dimensions; when
  disabled or unavailable, retrieval cleanly falls back to FTS-only (FEAT-025).
- Keep it in-stack/light per DISC-002's spirit (no heavy external vector DB —
  Variant D was rejected); cosine computed over stored BLOBs.

## Acceptance Criteria
1. New/consolidated memories get an `embedding`; a backfill populates existing rows;
   embedding failure leaves `NULL` and logs (no crash).
2. Vector candidates merge with FTS candidates and are reranked by activation
   (hybrid), improving recall on a keyword-mismatch fixture vs FTS-only.
3. With embeddings disabled/absent, retrieval falls back to FTS-only with no error.
4. Cosine search is unit-tested with fixed vectors (deterministic ordering).
