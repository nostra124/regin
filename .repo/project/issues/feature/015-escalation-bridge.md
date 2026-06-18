---
id: FEAT-015
type: feature
status: open
milestone: 0.3.0
disc: DISC-003
---
# FEAT-015 — Escalation bridge: problem → dvalin ticket (DISC-003 layer A)

Close the ops→dev loop: a regin `problem` gains an `escalate` action that emits
a **structured bus message** to a dvalin exec address requesting a BUG/FEAT, and
stores the returned ticket ref on the problem. (Pairs with dvalin 1.3.0 which
turns the escalation message into a ticket and reports status back.)
- `problem escalate <id> --as bug|feat --to <addr>` → structured message
  (kind=structured, typed escalation payload) via the bus client; record ref.
- escalation payload builder unit-tested; the round-trip to dvalin is the
  1.3.0 acceptance slice.

Acceptance: escalate builds the typed payload and sends it; the problem records
the escalation. Unit-tested.
