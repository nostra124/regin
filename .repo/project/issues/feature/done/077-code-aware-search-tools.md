---
id: FEAT-077
type: feature
priority: high
complexity: M
estimate_tokens: 30k-80k
estimate_time: 30-90min
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-077 — Code-aware search tools (glob + grep)

## Description

**As a** regin coding agent
**I want** dedicated `glob` and `grep` tools (backed by ripgrep)
**So that** I can search file contents and filenames efficiently without shelling out to `find`/`grep`

Currently regin's agent loop has only `bash` for search — the agent must construct
shell pipelines (`find . -name '*.rs'`, `grep -r 'pattern' src/`) which is
brittle, error-prone, and burns tokens on quoting. Dedicated tools give the LLM
structured, high-signal search.

## Acceptance Criteria

1. `glob(pattern: String, path: Option<String>)` tool returns matching file paths
   sorted by modification time, respecting `.gitignore` patterns (via ripgrep's
   ignore rules).

2. `grep(pattern: String, path: Option<String>, include: Option<String>)` tool
   returns matches with file path, line number, and surrounding line context;
   respects `.gitignore`.

3. Both tools are listed in the LLM's tool description alongside `bash`,
   `read_file`, `write_file`, `edit_file`, and `web_search`.

4. Both tools are registered in the daemon's tool dispatch (protocol.rs) and
   handled in the chat loop.

5. Error cases: invalid regex, non-existent path, permission denied — each returns
   a structured error message, not a panic.

6. Unit tests cover: glob success/error, grep success/error, `.gitignore`
   filtering, empty results.
