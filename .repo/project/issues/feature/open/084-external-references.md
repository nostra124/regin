---
id: FEAT-084
type: feature
priority: medium
complexity: S
estimate_tokens: 15k-30k
estimate_time: 15-30min
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-084 — External references

## Description

**As a** regin coding agent working in a monorepo or multi-repo project
**I want** to reference external directories and git repositories as additional
context
**So that** I can read API docs, shared libraries, and sibling projects without
leaving the current session

OpenCode's `references` config lets an agent access directories outside the
project worktree — either local paths or remote git repos that are cloned into
a cache. regin needs this to work effectively in multi-repo setups.

## Acceptance Criteria

1. External references are configured via SQLite:
   `regin config set references.<alias>.path /path/to/dir`
   `regin config set references.<alias>.repository "owner/repo"`

2. References with a `path` field are loaded directly. References with a
   `repository` field are shallow-cloned into regin's XDG cache directory
   (`~/.local/share/regin/references/<alias>/`).

3. A `repository` reference optionally accepts a `branch` field. If omitted,
   the default branch is used.

4. References are injected into the system prompt as additional context — the
   agent sees them as available directories it can read from using existing
   `read_file` and `glob` tools.

5. References are allowed through the tool permission boundary automatically
   (the agent can read any file inside a reference without additional prompts).

6. `regin config set references.<alias>.description <text>` adds a description
   that helps the LLM decide when to use a reference.

7. Unit tests cover: path resolution (relative, absolute, home-dir), repo
   shallow clone, reference listing, reference removal.

8. Integration test: `regin config set references.helpers.repository
   "example/helpers"` followed by `regin chat` — agent can list files in the
   cloned reference.
