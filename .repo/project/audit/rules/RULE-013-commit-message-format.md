# RULE-013 — Commit message format: one issue per commit

scope: full
severity: block

## Rule

Every commit must reference exactly one issue. The commit subject line
must follow the format:

```
TICKET-ID - Title - Verb
```

Where:
- **TICKET-ID** is `FEAT-NNN`, `BUG-NNN`, `DISC-NNN`, or `AUDT-NNN`
- **Title** is the ticket's own title (or a shortened form)
- **Verb** is a present-tense word describing what this commit does

## Valid verbs (non-exhaustive)

`Created`, `Implemented`, `Fixed`, `Tested`, `Refactored`,
`Documented`, `Closed`, `Promoted`, `Updated`, `Added`, `Removed`

## Examples

```
FEAT-037 - Workshop command via dwarf supervision - Implemented
FEAT-037 - Workshop command via dwarf supervision - Tested
BUG-008 - Fix bech32 encoding in bitcoin bin - Fixed
DISC-009 - Two interaction modes and workshop estimation - Created
AUDT-001 - Milestone 0.6.0 audit - Closed
```

## Exception: chore and hotfix

`chore/` and `hotfix/` branches may omit the ticket ID when no issue
applies:

```
chore - Expand .gitignore for editor artifacts
hotfix - Rotate leaked API key
```

## Multi-commit sequences

Multiple commits for the same ticket reuse the same prefix and vary
the verb. This makes `git log --oneline` tell the full story:

```
FEAT-040 - README and man page - Created
FEAT-040 - README and man page - Implemented
FEAT-040 - README and man page - Tested
```

## Pass criteria

- All commits on the branch (since branching from the integration
  base) have subjects matching `TICKET-ID - * - Verb` or the chore/
  hotfix exception.
- No commit subject uses the old colon format (`FEAT-NNN: ...`).
- No commit lacks a ticket ID entirely.

## Fail criteria

- Any commit subject that does not start with a valid `TICKET-ID - `
  prefix (or chore/hotfix exception).
- Commit subject uses the deprecated colon format.
- Two unrelated ticket IDs in one commit subject.

## Mechanical check

CHK-027 in `dvalin check` enforces this format on all unpushed commits.
The regex pattern is:

```
^(FEAT|BUG|DISC|AUDT)-[0-9]+ - .+ - \S+
^(chore|hotfix)( - .+)?
```

## Audit instruction

```bash
# Check all commits on this branch not yet on master
git log master..HEAD --format='%s' | grep -vE \
  '^(FEAT|BUG|DISC|AUDT)-[0-9]+ - .+ - \S+|^(chore|hotfix)'
```

Any output line is a violation. Fix by amending the commit message
before pushing (`git commit --amend` or interactive rebase).
