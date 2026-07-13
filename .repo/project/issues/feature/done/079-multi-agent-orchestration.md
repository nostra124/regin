---
id: FEAT-079
type: feature
priority: high
complexity: L
estimate_tokens: 80k-160k
estimate_time: 1.5-3h
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-079 — Multi-agent orchestration (subagent Task tool)

## Description

**As a** regin coding agent
**I want** to spawn child agent sessions (subagents) for parallel or specialised
subtasks
**So that** I can explore code, research dependencies, and build features
concurrently instead of sequentially

Currently regin runs a single agent loop — every tool call and LLM turn is
linear. OpenCode's Task tool lets a primary agent delegate work to subagents
with custom prompts, tools, and models. regin needs this to parallelise
exploration (e.g. `@explore` searches 3 locations at once) and to compose
workflows (research → plan → build → verify).

## Acceptance Criteria

1. The daemon exposes a `task(description, prompt, subagent_type)` tool that
   spawns a new child session on the same daemon, runs the prompt through the
   LLM with the specified agent persona, and returns the result.

2. Child sessions share the parent's tool set, memory context, and identity.db
   but have their own conversation history. They cannot spawn further children
   (one level of nesting — matches opencode's subagent model).

3. Three built-in subagent types:
   - **explore** — read-only (glob, grep, read_file only), for fast codebase
     search
   - **general** — full tool access, for general-purpose parallel tasks
   - **scout** — read-only + web_search + webfetch, for external research

4. Subagent types are defined in SQLite config (not hardcoded) — each has a
   system prompt override and a tool allowlist. Users can add custom types via
   `regin config set agent.<name> ...`.

5. Subagent results are returned as structured text in the Task tool's output.
   The primary agent decides how to incorporate the result — regin does not
   auto-merge.

6. Default concurrency limit: 3 simultaneous subagents. Configurable via `regin
   config set task.max_concurrency <N>`.

7. Unit tests cover: task spawn → completion, tool restriction per type,
   concurrency limit enforcement, error propagation from child session failure.
