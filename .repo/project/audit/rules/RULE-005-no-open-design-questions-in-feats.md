# RULE-005 — No open design questions in FEAT tickets

scope: full
severity: block

## Rule

A FEAT ticket entering implementation must have no unresolved design
questions. Open questions belong in DISC tickets, not FEAT tickets.

## Pass criteria

- No FEAT ticket in `phase: implement` or later contains: `TBD`, `TODO`,
  `FIXME`, `?` in a heading, or a non-empty `## Open questions` section.
- Every design question has been resolved and documented (or moved to a
  DISC ticket with `blocked_by:` set).

## Fail criteria

- A FEAT ticket contains `## Open questions` with content.
- A FEAT ticket heading contains a `?`.
- A FEAT ticket body contains `TBD` or `FIXME` outside of a code fence.

## Audit instruction

For each open FEAT ticket not in `done/`: scan for `TBD`, `FIXME`, `?`
headings, and non-empty `## Open questions` sections. List each finding
with the ticket ID and the offending line. For each: should this be
resolved now (blocking implementation) or filed as a DISC and deferred?
