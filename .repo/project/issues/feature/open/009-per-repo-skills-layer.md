---
id: FEAT-009
type: feature
priority: medium
complexity: M
phase: open
status: open
spawned_from: FEAT-008
depends_on: FEAT-008
---

# Per-repo skills layer in the XDG store (keyed by repo path)

## Description
**As** regin
**I want** per-repo *skills* stored in my XDG DB keyed by the repo path (like
per-repo context/memories from FEAT-008)
**So that** a repo can carry regin-specific skills without committing them, layered
on top of the system + user skill dirs.

Split out of FEAT-008, which delivered per-repo **context** + **memories** + the
repo-key resolver + legacy import. This ticket adds the **skills** tier.

## Implementation
- `repo_skills` table (repo_key, name, content, updated_at); db functions
  list/get/save/delete scoped by repo_key.
- Extend skill resolution so that, when operating in a repo, per-repo skills layer
  over user + system skills (user/per-repo override by name). Thread the resolved
  `repo_key` into the task-exec / skill-load path in `regind`.
- CLI: `regin task create … --repo` (or a `task add-repo`) to author a per-repo
  skill into the store; `task list` shows per-repo source.

## Acceptance Criteria
1. A per-repo skill is visible to `task list/exec` only when operating in that repo.
2. Per-repo skills do not leak to other repos; user/system skills still apply.
3. Round-trip unit tests for repo-scoped skill storage + resolution precedence.
