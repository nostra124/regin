---
id: FEAT-012
type: feature
status: done
milestone: 0.3.0
disc: DISC-004
---
# FEAT-012 — Local CLI-worker supervisor (claude/opencode)

The foreman relocates dvalin's stdin/stdout supervisor into the cave for the
pull-only CLI workers. Spawn a worker with an injected prompt, capture
stdout/stderr + exit, classify outcome (done/failed/empty).
- `worker::run(kind, prompt, cwd)` building the argv (`claude -p` / `opencode
  run`) — argv construction unit-tested; the spawn labeled needs-live (no CLI in
  CI).
- outcome classification unit-tested.

Acceptance: argv targets the right worker binary + prompt; outcome classifies
success/failure/empty. In-cave spawn needs live verification.
