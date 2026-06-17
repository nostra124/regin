# RULE-007 — Feature and backlog consistency

scope: full
severity: warn

## Rule

The set of open FEAT, BUG, and DISC tickets must be internally consistent:
references between tickets must resolve, milestone assignments must match the
active milestone, and implemented behaviour must match what tickets describe.

Inconsistencies left unresolved cause the conductor to select the wrong next
ticket, miss blocking relationships, or ship features that contradict each
other.

## Pass criteria

- Every `blocked_by:` value in any ticket resolves to an existing ticket file
  (open or in `done/`).
- Every ticket listed in `MILESTONE-X.Y.Z.md` under `tickets:` exists as a
  file in `feature/` or `feature/done/` (or `bug/` / `bug/done/`).
- No ticket in `feature/done/` or `bug/done/` appears in the active milestone's
  open ticket list.
- No two open FEAT tickets describe overlapping scope without a `blocked_by:`
  relationship between them.
- The `Cargo.toml` version matches the `configure.ac` version.
- No `BACKLOG.md` file exists (see RULE-009).

## Fail criteria

- A `blocked_by:` field names a ticket that does not exist anywhere.
- A milestone `tickets:` list contains a filename that cannot be found.
- A `done/` ticket is still listed as open in the active milestone's `tickets:` list.
- Two FEAT tickets implement the same function or CLI command without one
  superseding the other via `blocked_by:` or `supersedes:`.
- `Cargo.toml` and `configure.ac` versions differ.

## Audit instruction

1. **Dangling references**: for every `blocked_by:` in every ticket (open and
   done), verify the referenced ID exists. List any that are broken.

2. **Milestone consistency**: read the active `MILESTONE-X.Y.Z.md` ticket list.
   For each entry check it exists on disk. List missing files. For each ticket
   in `done/` check it is not still listed as open in the milestone. List any
   that are.

3. **Scope overlaps**: read all open FEAT ticket titles and one-line summaries.
   Identify any pair that could be implementing the same thing. For each overlap,
   determine if one ticket should be closed, merged into the other, or have a
   `blocked_by:` added.

4. **Version consistency**: compare `version = "..."` in `Cargo.toml` with
   `AC_INIT([dvalin], [version], ...)` in `configure.ac`. Report PASS or FAIL.

5. **Unassigned ticket hygiene**: scan `issues/feature/` and `issues/bug/` for
   ticket files that do not appear in any open milestone's `tickets:` list.
   For each: is it waiting for the next milestone planning session (acceptable),
   or has it been forgotten (needs a DISC to decide its fate)? List unassigned
   tickets older than the current milestone start date.

6. **Fixes**: for every FAIL finding above, propose the minimal concrete fix:
   - Update a `blocked_by:` to the correct ID.
   - Remove a ticket from the milestone list and note which `done/` file it maps
     to.
   - Add a `supersedes:` field to the surviving FEAT ticket.
   - Align `configure.ac` and `Cargo.toml` versions.

Report PASS only when all six checks are clear. Report FAIL with a numbered
list of findings and the proposed fix for each.
