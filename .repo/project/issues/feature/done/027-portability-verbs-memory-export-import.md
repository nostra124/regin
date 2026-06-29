---
id: FEAT-027
type: feature
priority: high
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.6.0
spawned_from: DISC-017
depends_on: FEAT-021
---

# FEAT-027 — Portability verbs: `regin memory export/import`

## Description
**As** an operator
**I want** to export and import the identity's memory store, with a stable identity
handle
**So that** I can move regin's identity between containers/machines without collisions
— the portability payoff of the memory plane.

## Implementation
- **Copy seam:** `identity.db` is a single copyable file (already true via FEAT-021);
  these verbs make moving it explicit and safe.
- **`regin memory export <path>`:** produce a self-contained, consistent snapshot of
  `identity.db` (e.g. SQLite backup API / `VACUUM INTO`), stamping
  `identity_meta.exported_from` (host) + timestamp.
- **`regin memory import <path>`:** load an exported store into this instance.
  Resolve identity handles via `identity_meta`:
  - `identity_id` (uuid) + `name` distinguish identities so **two identities never
    collide on one box** and **one identity can span boxes**.
  - On import, detect same-identity (merge/replace per flag) vs different-identity
    (refuse or adopt per flag); never silently clobber a different identity.
- `host`-scoped rows travel with the store but only inject on the matching host
  (FEAT-025), so importing onto a new box doesn't leak the old box's quirks.
- Surface `identity_meta` via `regin memory info` (identity_id, name, counts,
  schema_version).

## Acceptance Criteria
1. `memory export` produces a consistent, self-contained snapshot that re-opens
   cleanly and carries `identity_meta` (identity_id, name, exported_from).
2. `memory import` of a same-identity snapshot merges/replaces per flag; a
   different-identity snapshot is refused or adopted per explicit flag — never silent
   clobber.
3. Two distinct identities can coexist on one box without collision (distinct
   identity_ids).
4. Imported `host`-scoped rows inject only on the matching host.
5. Round-trip (export → import into a fresh instance) preserves all rows; unit-tested.
