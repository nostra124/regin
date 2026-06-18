# RULE-009 — No BACKLOG.md; all future work lives in DISC tickets

scope: full
severity: block

## Rule

`BACKLOG.md` must not exist in the repository. A flat backlog file is an
unstructured, unreviewed parking lot that bypasses the project methodology:
features added to it were never discussed with the user, never scoped into a
milestone, and never assigned a session. This creates invisible work that the
conductor cannot manage.

All future feature ideas, observations, and open questions must be captured as
DISC tickets in `discovery/`. A DISC ticket ensures the idea is discussed with
the user before any implementation decision is made. Only after that discussion
may a FEAT ticket be filed and added to a milestone.

## The correct flow

```
idea / observation
      ↓
  DISC-NNN-<slug>.md   (discovery/ — discuss with user)
      ↓
  decision: implement now or defer?
      ↓ implement now            ↓ defer
  FEAT-NNN filed             DISC stays open; revisit next milestone planning
  added to active milestone
      ↓
  session → done/
```

Nothing skips the DISC step. A FEAT ticket must always trace back to a DISC
or a BUG (bug tickets may be filed directly from observed failures).

## Pass criteria

- No file named `BACKLOG.md` (case-insensitive) exists anywhere in the
  repository tree.
- No file named `TODO.md`, `WISHLIST.md`, `PARKING-LOT.md`, or any variant
  serving the same purpose exists.
- Every open FEAT ticket can be traced to a closed or open DISC ticket via
  its `origin:` field or a `## Background` section referencing the DISC ID.
- All items that were previously in `BACKLOG.md` have been migrated: either
  to a DISC ticket (if the idea has not yet been discussed) or directly to a
  FEAT ticket added to a milestone (if the discussion already happened and the
  decision was to implement).

## Fail criteria

- `BACKLOG.md` exists at any path in the repository.
- Any markdown file at the repository root contains more than five `FEAT-` or
  `BUG-` references without itself being a milestone or issue file — this is a
  disguised backlog.
- A FEAT ticket has no traceable DISC origin and is not a direct consequence of
  a BUG report.

## Why backlogs accumulate

1. **Low-friction capture** — it is faster to append a line to `BACKLOG.md`
   than to open a DISC ticket. Over time the file grows unreviewed.
   Fix: the only low-friction capture allowed is a DISC ticket. The template is
   minimal: a one-line title and a `## Context` section.

2. **Deferred decisions** — items land in the backlog because a decision was
   not made at the time. Without a DISC ticket there is no record of why it was
   deferred or what information is still needed.
   Fix: open a DISC, mark it `status: open`, and add a `## Blocked on` section
   explaining what needs to happen before the decision can be made.

3. **Conductor bypass** — adding a FEAT directly to the backlog instead of a
   milestone means the conductor never sees it and it never gets done.
   Fix: if the feature is ready to implement, add it to the active milestone's
   `tickets:` list in the same commit as the FEAT ticket file.

## Audit instruction

1. **Existence check**: run `find . -iname 'backlog.md' -o -iname 'todo.md' -o -iname 'wishlist.md' -o -iname 'parking-lot.md'`
   (excluding `.git/`). Report PASS if empty; list every hit and mark FAIL.

2. **Disguised backlog scan**: for each `.md` file at the repository root that
   is not a milestone, issue, or rule file, count `FEAT-` and `BUG-` references.
   Any file with more than five such references without a `status:` frontmatter
   field is a probable disguised backlog. List it and ask the user to confirm.

3. **DISC traceability**: for each open FEAT ticket, look for an `origin:` field
   or a `## Background` / `## Context` section that names a DISC ticket ID.
   List any FEAT tickets with no traceable origin. For each, determine:
   - Was this discussed with the user? If yes, file a DISC ticket in `done/`
     to retroactively record the decision.
   - If not discussed, the FEAT ticket should be converted back to a DISC until
     the user has reviewed it.

4. **Migration of backlog items** (if BACKLOG.md was found):
   - For each item: create a `DISC-NNN-<slug>.md` in `discovery/` capturing the
     idea and its original context.
   - If the discussion has already happened (the item is well-understood and
     agreed), move it directly to a FEAT ticket and add it to the active
     milestone if it fits scope, or leave it as an open DISC for the next
     milestone planning session.
   - Delete `BACKLOG.md` once all items are migrated.
   - Commit the migration as a single atomic commit.

5. **Report**: state PASS or FAIL with a count of findings in each category.
   If FAIL, provide the migration plan as a numbered list.
