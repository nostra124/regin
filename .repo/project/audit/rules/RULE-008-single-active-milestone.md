# RULE-008 — Single active milestone; no cherry-picked future work

scope: full
severity: block
gate_of: dev-pipeline

## Rule

At any point in time exactly one milestone may carry `status: active`.
All implementation work on a branch must belong to that milestone's ticket list.
No ticket from a future milestone may be implemented while the active milestone
is incomplete, and no ticket inside the active milestone may be left partially
implemented when a session ends.

This rule enforces the "one session = one ticket to done/" invariant across the
whole project lifecycle.

## Definitions

- **Active milestone**: the single `MILESTONE-X.Y.Z.md` file with `status: active`.
- **Future milestone**: any milestone file with a higher version number than the
  active one.
- **Partial implementation**: a ticket that is neither fully in `done/` nor
  cleanly in the open queue — i.e., code changes exist for it but the ticket
  has not been moved to `done/`.
- **Cherry-picked future work**: a commit or code change that implements scope
  described in a future-milestone ticket while the active milestone still has
  open tickets.

## Pass criteria

- Exactly one `MILESTONE-X.Y.Z.md` file has `status: active`.
- Every open FEAT or BUG ticket whose code changes appear in `git log` since
  the milestone was activated belongs to the active milestone's `tickets:` list.
- No future-milestone ticket has any corresponding code change in the branch
  history.
- Every ticket that has code changes is either fully in `done/` or fully open
  (no partial states: code committed but ticket not in `done/`, or ticket in
  `done/` but code not merged).
- The active milestone's `tickets:` list contains no ticket that is also listed
  in a future milestone's `tickets:` list.

## Fail criteria

- More than one milestone file has `status: active`.
- A commit message or changed file can be attributed to a future-milestone
  ticket (e.g., references `FEAT-NNN` where NNN is only listed in a later
  milestone).
- A ticket exists in `feature/` (open) but `git log` shows committed code
  implementing it — the session ended without moving it to `done/`.
- A ticket appears in both the active and a future milestone's `tickets:` list.
- Any milestone has `status: active` and `phase: stable` simultaneously (stable
  milestones must have `status: done`).

## Why these violations occur

1. **Scope creep during a session** — while fixing FEAT-A a developer notices
   adjacent work from FEAT-B (future milestone) and implements it "while there".
   Fix: scope boundary is the ticket. Anything outside the ticket is a new
   sub-issue; file it and defer or block per the sub-issue handling rule.

2. **Premature session close** — a session ends (container stops, agent exits)
   before the ticket reaches `done/`. Code is committed but the ticket file
   was not moved.
   Fix: `dvalin conduct` already checks `ticket_is_done()` post-session and
   exits with code 2 if the ticket was not moved. The CI gate must enforce this.

3. **Milestone list not maintained** — a new ticket is filed and implemented
   without being added to the active milestone's `tickets:` list, making it
   invisible to the conductor.
   Fix: filing a ticket must always be accompanied by adding it to the active
   milestone file in the same commit.

4. **Parallel milestone activation** — a second milestone is activated before
   the first reaches `stable`, typically to "start planning". Planning belongs
   in DISC tickets, not in a second active milestone.
   Fix: only `dvalin release --promote` may set a milestone to `active`; it
   also sets the previous milestone to `done`.

## Audit instruction

1. **Active milestone count**: count all `MILESTONE-*.md` files with
   `status: active`. Report PASS if exactly one; FAIL with names if not.

2. **Stable + active conflict**: check whether any active milestone also has
   `phase: stable`. A stable milestone must have `status: done`. Report any
   conflict.

3. **Ticket list membership**: for each commit on this branch since the active
   milestone was last modified, extract referenced ticket IDs (from commit
   messages and changed filenames). Verify every ID appears in the active
   milestone's `tickets:` list. List any that do not.

4. **Future-milestone leakage**: read every future milestone's `tickets:` list.
   Check whether any of those ticket IDs appear in the git log for the current
   branch. List any matches as cherry-pick violations.

5. **Partial implementation check**: for each open ticket in `feature/` and
   `bug/`, check whether `git log --all -S <ticket-id>` returns any commit.
   If commits exist, the ticket has code but is not in `done/` — report as
   partial. For each partial: decide whether to complete it now (move to `done/`
   after verifying tests) or revert the code changes.

6. **Duplicate ticket across milestones**: for each ticket ID in the active
   milestone's list, check all other milestone files for the same ID. Report
   any duplicate as a conflict to be resolved by removing it from the future
   milestone and filing a new ticket when the time comes.

7. **Fixes**: for each FAIL finding propose the exact remediation:
   - Set excess active milestones to `status: done` or `status: planned`.
   - Move the partial ticket to `done/` after a remediation session, or revert
     and file a clean BUG ticket.
   - Remove the cherry-picked commit from the branch (revert or squash) and
     file the work as a future-milestone FEAT ticket.
   - Remove duplicate IDs from future milestones.

Report PASS only when all seven checks are clear.
