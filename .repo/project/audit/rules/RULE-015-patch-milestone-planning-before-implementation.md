# RULE-015 — Plan the patch milestone before implementing patch fixes

scope: full
severity: block
gate_of: dev-pipeline

## Rule

Bug-fix (patch) work is planned before it is implemented, exactly as feature
milestones are. Before any code is written for a patch:

1. A **patch milestone** file exists (`MILESTONE-<x>.<y>.<z>.md`, `z > 0`) with
   `kind: patch`, a goal, the **ordered** list of BUG tickets it ships, and exit
   criteria.
2. Every BUG in that patch carries `milestone: <x>.<y>.<z>` pointing at it.
3. **Multiple bugs may share one patch version**, but each bug is implemented as
   **exactly one commit** (`BUG-NNN - <title> - Implemented`). One version, many
   bugs, one commit per bug.
4. The version bump to `<x>.<y>.<z>` in `Cargo.toml` happens as part of cutting
   the patch (via `dvalin release --patch`), not ad hoc mid-implementation.

A patch that starts implementation without this plan is out of process — the
same "plan before build" gate RULE-010 applies to feature milestones.

## Pass criteria

- The active patch milestone file lists its bugs in dependency order with exit
  criteria, and every listed bug points back at it.
- Each shipped bug corresponds to one commit referencing that bug id.
- `Cargo.toml` is bumped to the patch version when the patch is cut, once.

## Fail criteria

- Patch bug-fix commits exist with no patch-milestone file, or bugs whose
  `milestone:` does not match the patch they shipped in.
- A single commit bundles multiple bugs, or one bug is spread across commits
  that each claim to "implement" it.
- The version is bumped per-bug, or not bumped at all when the patch is cut.

## Audit instruction

1. For the active/just-closed patch, confirm a `kind: patch` milestone file with
   an ordered bug list and exit criteria exists, and that each bug's
   `milestone:` matches. Report PASS/FAIL.
2. Walk the patch's commits: each bug maps to exactly one `BUG-NNN ... Implemented`
   commit; no commit bundles bugs. Flag violations.
3. Confirm `Cargo.toml` was bumped to the patch version exactly once at cut time.
