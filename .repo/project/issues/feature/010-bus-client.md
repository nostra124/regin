---
id: FEAT-010
type: feature
status: open
milestone: 0.3.0
disc: DISC-004
---
# FEAT-010 — regind messaging-bus client (identity, inbox/outbox, two modes)

regin's half of the dvalin bus. execd drops bus mail into a cave inbox file and
drains a cave outbox file (dvalin FEAT-124). This is the in-cave client:
- identity `role@cave` (from `REGIN_ADDRESS` / config).
- `inbox(unread)` reads the inbox JSONL (default `/var/lib/regin/inbox.jsonl`,
  `REGIN_INBOX` override); marks read via an offset cursor.
- `send(to, kind, body, ref_id)` appends a line to the outbox JSONL
  (`/var/lib/regin/outbox.jsonl`, `REGIN_OUTBOX`) for execd to relay.
- two modes: `unstructured` | `structured` (typed JSON body).
- CLI `regin bus send|inbox`; protocol Request/Response.

Acceptance: send writes a well-formed outbox line stamped with our address;
inbox reads only undelivered lines and advances the cursor. Unit-tested over
temp files.
