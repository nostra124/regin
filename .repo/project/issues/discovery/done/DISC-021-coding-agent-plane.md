---
id: DISC-021
type: discovery
priority: high
status: done
complexity: XL
spawned_features:
  - FEAT-077
  - FEAT-078
  - FEAT-079
  - FEAT-080
  - FEAT-081
  - FEAT-082
  - FEAT-083
  - FEAT-084
  - FEAT-085
---

# DISC-021 — Coding agent plane (repo-worker)

## Describe

regin is currently an **operations agent** — it administers Linux servers via shell
commands, scheduled skills, and ITIL incident/change/problem records. It has no
ability to **write, refactor, or develop software** inside a repository. The
foreman/repo-worker plane was identified in the operator-model workshop but
deferred (see `.repo/dvalin/notes.md` § "Two planes").

Meanwhile, opencode (the reference coding agent) has a mature feature set for
autonomous software development: code-aware search (glob/grep), LSP diagnostics,
multi-agent orchestration, MCP extensibility, plugins, and multi-provider model
support. regin needs these to act as an autonomous coding agent.

The question: which of opencode's features should regin adopt, in what order,
and at what integration depth?

## Variants considered

| Variant | Summary | Key trade-off |
|---------|---------|---------------|
| A | **Minimal path** — only code-aware search (glob+grep) + LSP diagnostics. Unlocks basic "edit, compile, fix" cycle | Fast to ship but no multi-agent or extensibility |
| B | **Full coding agent** — glob/grep + LSP + subagents + MCP + permissions + plugins + multi-model + references | Most capable but largest implementation surface; some features (plugins, MCP) are XL alone |
| C | **Variant B, sliced by dependency order** — foundation tools first, then orchestration, then extensibility | Longer delivery but each slice is independently shippable and testable |

## Decision matrix

| Criterion | Weight | A | B | C |
|-----------|--------|---|---|---|
| Unlocks "edit, compile, fix" loop | high | ✓ | ✓ | ✓ |
| Parallel work (multi-agent) | med | ✗ | ✓ | ✓ (later slice) |
| Third-party tool ecosystem | med | ✗ | ✓ | ✓ (later slice) |
| Implementation risk | high | low | high | med |
| Time-to-first-value | high | weeks | months | weeks (foundation slices first) |

## Arguments

### Pro — Variant C (sliced delivery)

- **Foundation first (tools + LSP)** is independently useful — even without
  subagents, regin can `grep` → `edit` → `cargo check` → fix. This closes the
  basic coding loop.
- **Subagent orchestration** builds naturally on the existing session/socket
  protocol (FEAT-023 already does session management). The Task tool is a
  lightweight extension of `session_create`/`session_prompt`.
- **MCP + plugins** are the highest-value extensibility surface but also the
  largest design surfaces. Landing them after the agent loop is proven avoids
  building the wrong abstraction.
- **Multi-model abstraction** is an orthogonal concern — regin currently bakes
  in NanoGPT's HTTP API. A trait-based provider model (like opencode's
  provider routing) lets users bring their own LLM without forking.
- **Variant A alone is insufficient** — without subagents and MCP, regin can
  edit code in a repo but cannot parallelise exploration, run multi-step
  workflows, or integrate with external tool ecosystems.

### Con / risks

- **Slice boundaries must be clean.** A badly chosen seam between FEATs creates
  rework. The proposed slice boundaries (tools → LSP → subagents → permissions →
  MCP → plugins → multi-model) are aligned with the existing regin architecture:
  each adds capabilities to the agent loop without changing the core protocol.
- **Plugin system is inherently large.** OpenCode's plugin system is ~20 event
  hooks across sessions, tools, files, LSP, permissions, and TUI. regin's
  equivalent can be scoped to tool events first, with session/file events added
  later.
- **MCP is a protocol spec, not a library.** regin would need to implement the
  MCP transport (stdio/SSE) and tool discovery handshake in Rust. Several Rust
  MCP crates exist (`mcp-client`, `mcp-server`) — evaluating vs. building is a
  design sub-decision.

## Decision

**Chosen:** Variant C — full coding agent plane, sliced into 9 FEATs in
dependency order.

**Why:** Variant A is too narrow (no multi-agent, no extensibility). Variant B
delivers everything but ships nothing until everything is done. Variant C gives
us a shippable foundation after the first 2-3 FEATs while keeping the full
target in sight. The 9 FEATs are:

1. **FEAT-077** — Code-aware search (glob + grep tools via ripgrep)
2. **FEAT-078** — LSP diagnostics feedback loop
3. **FEAT-079** — Multi-agent orchestration (subagent Task tool)
4. **FEAT-080** — Granular tool permissions (allow/ask/deny)
5. **FEAT-081** — MCP client protocol (local + remote servers)
6. **FEAT-082** — Plugin system (event-driven hooks)
7. **FEAT-083** — Multi-provider model abstraction
8. **FEAT-084** — External references (local dirs + git repos)
9. **FEAT-085** — Edit tool polish (apply_patch, undo/redo)

Order rationale: tools → LSP is the minimal "edit, compile, fix" loop.
Subagents use those tools in parallel. Permissions gate them safely. MCP and
plugins extend the tool ecosystem. Multi-model and references are additive
quality-of-life features. Edit polish is independent and can land anywhere.

## Spawned features

(To be filled on close — see FEAT-077..085 under `issues/feature/open/`)
