---
id: FEAT-008
type: feature
priority: medium
complexity: M
estimate_tokens: 40k-80k
estimate_time: 60-120min
phase: done
status: done
---

# Per-repo additions (context / memories / skills) in regin's XDG store, keyed by repo path

## Description
**As** regin
**I want** my per-repo additions — extra context/instructions, memories, and
special skills for a given repository — stored in my own XDG store keyed by the
repo's filesystem path
**So that** regin's knowledge about a repo travels with regin (not committed into
the repo), consistent with regin's "all state in SQLite, no config files" model.

Boundary (settled with René):
- **dvalin's** workflow-engine instructions live **in** the repo under
  `.repo/dvalin/` (+ the methodology under `.repo/project/`).
- **regin's** own additions live **outside** the repo, in regin's XDG store,
  **keyed by the repo path** (the repo's filesystem path is the identifier).

This retires the current in-repo `.repo/regin/context.md` mechanism.

## Implementation
- **Repo identity:** resolve cwd → repo root (git toplevel if present, else the
  directory), canonicalize the path; that path string is the repo key.
- **Storage (SQLite):** scope per-repo data by repo key:
  - memories — add a `repo_key` (nullable = global) to `memories`; per-repo
    memories load only when operating in that repo, in addition to globals.
  - skills — allow a per-repo skill layer resolved from the store for the
    current repo key (on top of system + user skills).
  - context/instructions — a per-repo context blob keyed by repo key (replaces
    reading `.repo/regin/context.md`).
- **Context loader (`regin-core/src/context.rs`):** stop reading
  `<cwd>/.repo/regin/context.md`; instead load the per-repo context + memories +
  skills for the resolved repo key from the store. Keep a one-time **import** of
  a legacy `.repo/regin/context.md` if found (migration), then ignore it.
- **CLI:** let the user manage per-repo additions, e.g. `regin memory save … `
  gains an implicit/explicit repo scope when run inside a repo; a way to view
  what regin knows about the current repo (`regin context show` or similar).
- Update docs (profile.md, README, the `chat` long-help in `regin-cli`) to the
  new model.

## Acceptance Criteria
1. Operating regin inside a repo loads that repo's context + per-repo memories +
   per-repo skills from the XDG store, resolved by the canonical repo path.
2. Nothing regin-specific is written into the repo working tree (no
   `.repo/regin/`).
3. A legacy `.repo/regin/context.md`, if present, is imported once then ignored.
4. Per-repo memories/skills do not leak into other repos; globals still apply
   everywhere.
5. Round-trip unit tests for repo-key resolution + per-repo memory/skill scoping;
   docs updated.

## Delivered vs. split
This ticket delivered per-repo **context** + **memories** + repo-key resolution +
legacy `.repo/regin/context.md` import + the `regin context` CLI. Per-repo
**skills** layering was split into **FEAT-009**.
