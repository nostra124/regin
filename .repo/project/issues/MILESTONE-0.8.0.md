---
id: MILESTONE-0.8.0
type: milestone
status: done
depends_on: [MILESTONE-0.7.0]
---

# Milestone 0.8.0 — Coding agent plane (repo-worker)

Turns regin from a **server operations agent** into a **full-stack autonomous
coding agent** that can work inside a repository: search code, read LSP
diagnostics, spawn subagents for parallel work, and extend itself via MCP tools
and plugins. This is the **foreman/repo-worker plane** identified in the
operator-model workshop (see `.repo/dvalin/notes.md` § "Two planes") — regin
working *inside a repo* under that repo's methodology.

Derived from **DISC-021** (coding agent) and **DISC-022** (web UI). Builds on the
identity plane (0.6.0: persona, memory, soul) for agent identity and value-gated
decisions. **Does not depend on** MILESTONE-0.7.0 (intent/planning) — all tracks
in 0.8.0 are orthogonal to objectives/planning, though they may converge later.

## Tickets

### Track A — Coding agent (DISC-021)

| ID | Title | Complexity | From | Status |
|----|-------|-----------|------|--------|
| FEAT-077 | Code-aware search tools (glob + grep) | M | DISC-021 | done |
| FEAT-078 | LSP diagnostics feedback loop | L | DISC-021 | done |
| FEAT-079 | Multi-agent orchestration (subagent Task tool) | L | DISC-021 | done |
| FEAT-080 | Granular tool permissions (allow/ask/deny) | M | DISC-021 | done |
| FEAT-081 | MCP client protocol (local + remote) | L | DISC-021 | done |
| FEAT-082 | Plugin system (event-driven hooks) | L | DISC-021 | done |
| FEAT-083 | Multi-provider model abstraction | L | DISC-021 | done |
| FEAT-084 | External references (local dirs + git repos) | S | DISC-021 | done |
| FEAT-085 | Edit tool polish (apply_patch, undo/redo) | S | DISC-021 | done |

### Track B — Web UI (DISC-022)

| ID | Title | Complexity | From | Status |
|----|-------|-----------|------|--------|
| FEAT-087 | Web UI server (embedded HTTP, PAM auth, mobile-first SPA) | M | DISC-022 | done |

## Suggested delivery order

### Track A

1. **Foundation** — FEAT-077 (glob+grep) · FEAT-085 (edit polish) together form
   the basic "search, edit, undo" coding loop. Independent of each other; can
   land in parallel.

2. **Quality feedback** — FEAT-078 (LSP diagnostics). Closes the
   edit→compile→fix cycle. Depends on FEAT-077 (grep needed to navigate to
   error locations).

3. **Orchestration** — FEAT-079 (subagents). Uses the tools from FEAT-077/078 in
   parallel. Depends on nothing beyond the existing session protocol.

4. **Safety** — FEAT-080 (permissions). Gates all tools, including those from
   earlier FEATs. Can land any time after the first coding session proves the
   need; logically late because you need something worth gating first.

5. **Extensibility** — FEAT-081 (MCP) · FEAT-082 (plugins). Both extend the
   tool ecosystem. MCP is higher-value first (existing ecosystem of servers);
   plugins are more general. Can land in parallel.

6. **Quality-of-life** — FEAT-083 (multi-model) · FEAT-084 (references).
   Independent additive features; land any time after foundation.

### Track B

FEAT-087 (web UI) is independent of Track A. It can land any time:

7. **Web UI** — FEAT-087. Embedded HTTP server in `regind`, PAM auth,
   mobile-first SPA, enabled via `regin webui enable`.

## Exit criteria

- regin can search a codebase with `glob` and `grep` without shelling out to
  bash; results respect `.gitignore`.
- After every edit, LSP diagnostics are fed back into the agent loop;
  `cargo check`-style manual verification is no longer required for basic
  error feedback.
- regin can spawn subagent sessions (`explore`, `general`, `scout`) for
  parallel work; concurrency is bounded and configurable.
- Every tool has an `allow`/`ask`/`deny` permission gate; `bash` supports
  per-command glob patterns; the CLI renders `ask` prompts inline.
- MCP servers (local stdio and remote SSE) can be connected; their tools
  appear in the LLM's tool set under `mcp_<name>_<tool>`.
- Plugins (compiled Rust dylibs) can hook into tool execution lifecycle;
  plugin panics do not crash the daemon.
- LLM provider is abstracted behind `LlmClient` trait; any OpenAI-compatible
  endpoint is configurable via `llm.base_url`/`llm.api_key`/`llm.model`.
- External directories and git repos can be referenced and their files read
  by the agent.
- `apply_patch` and `undo`/`undo_list` tools are available for precise,
   revertable editing.
- `regin webui enable` starts an embedded HTTP server in `regind` on
   `127.0.0.1:<port>` (default 8080). Three URL namespaces: **public `/`**
   (no auth, landing page), **public `/artifacts`** and **`/repo`**
   (build outputs and package repos, world-readable, paths configurable),
   and **authenticated `/regin/*`** (PAM auth gates SPA and REST API).
- The SPA has a **multi-tab interface** with three built-in tab types:
   **chat** (`💬`, directory-scoped), **terminal** (`🖥`, PTY over WebSocket),
   and **goal** (`🎯`, autonomous multi-step objective with plan checklist).
- regin can register **dashboard tabs** at runtime via
   `POST /regin/api/tabs/register` — the agent can generate its own UI tabs
   (statistics, controls, etc.) without a rebuild.
- **Build artifacts** (`.deb`, `.rpm`, etc.) are served for download at
   `/artifacts/<type>/<file>` with directory listing (public, no auth).
- **Package repositories** available at `/repo/apt/` (APT) and
   `/repo/rpm/` (RPM) — valid repo metadata generated by regin (public, no auth).
- Artifact and repo paths are configurable via `webui.public.*` settings;
   can be moved under `/regin/*` to make them private if desired.
- `profile.md` documents the `libpam` build dependency.
- 100% test coverage on all new code; no open design questions in any 0.8.0
   FEAT (RULE-005).

## Status: done

Both tracks are complete (FEAT-077 through FEAT-085, FEAT-087). This closes
out the last open milestone on the roadmap — see `.repo/dvalin/notes.md`
for the FEAT-087 close-out entry documenting the scope decisions made along
the way (goal-loop reuse instead of the dormant 0.7.0 pipeline, hand-rolled
PAM FFI, the auth-boundary reconciliation, and the musl/PAM packaging
constraint).
