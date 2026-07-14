---
id: FEAT-081
type: feature
priority: medium
complexity: L
estimate_tokens: 80k-160k
estimate_time: 1.5-3h
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-081 — MCP client protocol (local + remote servers)

## Description

**As a** regin coding agent
**I want** to connect to Model Context Protocol (MCP) servers for additional
tools (database queries, API integrations, file system operations, etc.)
**So that** I can extend regin's capabilities without modifying regin's core
tool set

MCP is the open standard for LLM tool interoperability. OpenCode supports both
local MCP servers (stdio) and remote MCP servers (SSE/HTTP). regin should speak
the same protocol so users can plug in the same ecosystem of MCP tools.

## Acceptance Criteria

1. MCP client implemented in Rust (or wrapping a Rust MCP crate such as
   `mcp-client` or `rmcp`). Supports:
   - **Local** servers: spawned as a child process, communicated over stdio
     (JSON-RPC 2.0)
   - **Remote** servers: SSE/HTTP transport with optional auth headers

2. MCP servers are configured via SQLite store (not a file):
   `regin config set mcp.<name>.type local|remote`
   `regin config set mcp.<name>.command ["npx", "-y", "server"]`
   `regin config set mcp.<name>.url "https://..."`

3. On daemon startup (and config change), configured MCP servers are connected
   and their tool list is fetched via `tools/list`. Each MCP tool is registered
   in the LLM's tool set with a `mcp_<name>_<toolName>` prefix.

4. Tool calls are dispatched to the correct MCP server via `tools/call` with
   proper argument serialization. Responses are returned as tool results.

5. MCP server lifecycle: connect on daemon start, reconnect with exponential
   backoff on failure (max 5 retries), disconnect on daemon shutdown.

6. Timeout per MCP tool call: default 30s, configurable per-server.

7. MCP tools respect the same permission system (FEAT-080) — they can be
   allowed/asked/denied by pattern (`mcp_myserver_*`).

8. Unit tests cover: stdio transport, JSON-RPC handshake, tool discovery,
   tool call round-trip, error handling (server crash, timeout, invalid JSON).
