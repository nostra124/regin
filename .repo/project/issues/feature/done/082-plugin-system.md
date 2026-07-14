---
id: FEAT-082
type: feature
priority: medium
complexity: L
estimate_tokens: 100k-200k
estimate_time: 2-4h
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-082 — Plugin system (event-driven hooks)

## Description

**As a** regin user
**I want** to write and load plugins that hook into regin's agent loop events
(tool execution, session lifecycle, file changes)
**So that** I can customise behaviour (e.g. log all tool calls, inject env vars,
block sensitive file reads) without modifying regin's source

OpenCode's plugin system fires events at key lifecycle points and lets external
scripts intercept, modify, or block them. regin needs a similar mechanism for
extensibility.

## Acceptance Criteria

1. Plugin host: WASM-based plugins (wasmtime runtime) scoped to the operator
   plane, or a simpler Rust trait-based plugin loading from a configurable
   directory (`~/.config/regin/plugins/`). The simpler trait-based approach is
   preferred for v1.

2. Plugin lifecycle hooks (v1):
   - `tool.execute.before` — intercept tool calls; can modify args or reject
   - `tool.execute.after` — observe tool results; can modify response
   - `session.created` — hook on new chat session
   - `session.compacting` — inject context into compaction summaries

3. Plugins are `.so`/`.dylib` files (compiled Rust dylibs) placed in:
   - User: `~/.config/regin/plugins/`
   - System: `/usr/share/regin/plugins/`

4. Plugin API is a single exported function:
   ```rust
   #[no_mangle]
   pub extern "C" fn regin_plugin_init() -> Box<dyn Plugin>
   ```
   where `Plugin` is a regin-core trait with hook callbacks.

5. Plugin loading errors (missing symbol, version mismatch, panic during init)
   are logged but do not crash the daemon. A plugin that panics during hook
   execution is disabled for the remainder of the session.

6. Plugins are enabled/disabled via SQLite:
   `regin config set plugin.<name>.enabled true|false`

7. Unit tests cover: plugin load, hook invocation, hook error handling
   (panic → disable, reject → block tool), version mismatch detection.

**Note:** WASM-based plugin hosting is deferred to a follow-up FEAT if demand
warrants. The trait-based dylib approach matches regin's existing Rust ecosystem
and avoids adding a WASM runtime dependency.
