---
id: FEAT-078
type: feature
priority: high
complexity: L
estimate_tokens: 80k-150k
estimate_time: 1.5-3h
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-078 — LSP diagnostics feedback loop

## Description

**As a** regin coding agent
**I want** Language Server Protocol integration that feeds compiler/linter
diagnostics back into the agent loop after edits
**So that** I can auto-detect and fix errors without waiting for the user to
point them out

After every edit, the agent currently must manually run `cargo check` (or
equivalent) via `bash` to see if the code compiles. An LSP client lets regin
receive diagnostics automatically after file changes, making the edit→verify→fix
cycle much tighter.

## Acceptance Criteria

1. LSP client library integrated (e.g. `tower-lsp`, or a lightweight LSP
   handshake over stdio). Language servers are auto-detected by file extension
   (rust-analyzer for `.rs`, typescript-language-server for `.ts`, etc.).

2. After a `write_file` or `edit_file` tool call completes, the daemon triggers
   LSP diagnostics for the affected file and returns any errors/warnings as
   structured data appended to the tool result.

3. Diagnostics are deduplicated and debounced (500ms window) to avoid flooding
   the context with repeated errors.

4. The agent can also request diagnostics on-demand via a `diagnostics` tool
   (e.g. `diagnostics(path: "src/main.rs")`).

5. Language server processes are spawned on-demand and recycled after a configurable
   idle timeout (default 5 min) to avoid resource leaks.

6. Supported out of the box: rust-analyzer (Rust) and typeScript-language-server
   (TypeScript/JavaScript). Others are configurable via `regin config set
   lsp.<lang>.command ...`.

7. LSP is opt-in (`regin config set lsp.enabled true`). When disabled, behaviour
   is identical to today (no LSP processes, no diagnostics in tool results).

8. Unit tests cover: LSP handshake, diagnostics parse, debounce timing,
   process lifecycle (spawn → idle → kill).
