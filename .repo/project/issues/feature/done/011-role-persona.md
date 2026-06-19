---
id: FEAT-011
type: feature
status: done
milestone: 0.3.0
disc: DISC-005
---
# FEAT-011 — Role-persona config + capability(=tool) ceiling

A regin instance *becomes* a role (cfo, dev-lead, …). A persona declares its
role id, system-prompt preamble, and an allowed **capability/tool ceiling**.
- `persona.toml` loader (role, title, prompt, tools[]).
- capability ceiling: the tool dispatcher refuses a tool not in the persona's
  allowed set (authorization ceiling — least privilege).
- CLI `regin persona show`; persona feeds the chat/agentic system prompt.

Acceptance: a persona loads and round-trips; a tool outside the ceiling is
refused; an allowed tool passes. Unit-tested.
