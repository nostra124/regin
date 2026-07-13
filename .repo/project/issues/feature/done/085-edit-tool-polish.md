---
id: FEAT-085
type: feature
priority: low
complexity: S
estimate_tokens: 10k-20k
estimate_time: 10-20min
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-085 — Edit tool polish (apply_patch, undo/redo)

## Description

**As a** regin coding agent
**I want** an `apply_patch` tool for diff-based edits and an `undo` mechanism
for reverting changes
**So that** I can make precise, reviewable changes and roll back mistakes without
manual git operations

OpenCode provides `apply_patch` (applies a unified diff) and `/undo`/`/redo`
commands. regin's current `edit_file` is line-based and does not support
undo. Adding patch-based editing and undo gives the agent more precise control
and a safety net.

## Acceptance Criteria

1. `apply_patch(tool: "write"|"edit"|"delete", path: String, patch: String)`
   tool accepts a unified-diff-format patch string, applies it to the target
   file, and returns the result.

   - `write`: creates a new file with content from the patch
   - `edit`: applies the patch to an existing file
   - `delete`: deletes the file

2. Before every `write_file`, `edit_file`, or `apply_patch` call, the daemon
   snapshots the affected file's current content in memory (ring buffer, last
   50 edits per file).

3. An `undo` tool reverts the most recent edit to a file. An `undo_list` tool
   shows recent edits (file path, timestamp, short description).

4. Undo state is ephemeral (in-memory, lost on daemon restart). It is NOT a
   git commit or a backups mechanism.

5. Unit tests cover: patch application (create, edit, delete), malformed patch
   rejection, undo/redo round-trip, snapshot buffer eviction.
