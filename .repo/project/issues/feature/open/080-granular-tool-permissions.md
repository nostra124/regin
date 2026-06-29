---
id: FEAT-080
type: feature
priority: high
complexity: M
estimate_tokens: 30k-70k
estimate_time: 30-90min
phase: open
status: open
spawned_from: DISC-021
---

# FEAT-080 — Granular tool permissions (allow / ask / deny)

## Description

**As a** user running regin as a coding agent
**I want** per-tool permission gates (allow / ask / deny) with optional
bash-command glob matching
**So that** I can restrict dangerous operations (e.g. `git push`, `write_file` to
sensitive paths) while allowing routine ones

Currently every tool is unconditionally allowed. This is fine for a trusted
operator but risky for an autonomous coding agent that may take unexpected
actions. OpenCode's permission model gates each tool with three states — allow,
ask (prompt user), deny — and supports glob patterns for bash commands.

## Acceptance Criteria

1. Each tool (`bash`, `read_file`, `write_file`, `edit_file`, `glob`, `grep`,
   `web_search`, `webfetch`, `task`) has a permission level stored in SQLite
   config: `"allow"` | `"ask"` | `"deny"`.

2. `bash` permissions support glob patterns on the command string:
   - `{"*": "allow", "git push *": "ask", "rm -rf *": "deny"}` — last match wins.

3. When a tool is `deny`: the daemon returns an error message and does not
   execute the tool. The LLM sees "Tool X is disabled by policy."

4. When a tool is `ask`: the daemon blocks execution, sends a permission request
   to the CLI client, which prompts the user (Y/n). The user's response is
   returned to the daemon; if denied, the tool is not executed.

5. Permission defaults: all tools `"allow"` (backward compatible). `ask`/`deny`
   are opt-in via `regin config set permission.<tool> <level>`.

6. The CLI client renders permission prompts as inline TUI dialogs without
   breaking the streaming chat display.

7. Unit tests cover: allow/deny/ask dispatch, bash glob matching (literal,
   wildcard, prefix), permission cache invalidation on config change.
