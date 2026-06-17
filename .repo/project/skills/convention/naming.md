# Branch + commit + PR naming

> Structural conventions. Reviewed for fit; not gate-enforced.
> Compare with `policy/` (binding) and `skills/language/` (idiom).

## Branch names

Branches follow a milestone-prefixed scheme so the active worktable
groups by version and old branches sweep away when a milestone
closes. The current milestone is the one whose `MILESTONE-<x.y.z>.md`
file lives at `.repo/project/issues/`.

```
m<x.y.z>/feat-<NNN>-<slug>      one feature ticket
m<x.y.z>/bug-<NNN>-<slug>       one bug ticket
m<x.y.z>/disc-<NNN>-<slug>      one discovery ticket
m<x.y.z>/feat-<NNN>-<NNN>-<slug>  multi-ticket PR (rare)
m<x.y.z>/plan                   milestone planning (writing the plan file)
m<x.y.z>/close                  milestone closure (delete plan, bump VERSION)
chore/<slug>                    out-of-ticket housekeeping
hotfix/<slug>                   emergency bypassing milestone scope
```

Examples (taken from this repo's history):

```
m2.5.0/feat-199-source-builder-removal
m2.5.0/feat-200-build-cycle-frontend
m2.5.0/feat-202-203-design-test-sessions
m2.5.0/feat-160-204-207-close-sweep
m2.5.0/plan
m2.5.0/close
chore/branch-naming-convention
chore/repo-project-layout-refactor
hotfix/cve-2026-12345
```

### Rationale

- **Milestone prefix.** `git branch --list 'm2.5.0/*'` shows the
  whole worktable for a release. Closing the milestone deletes the
  matching branches in one sweep.
- **`m` not `milestone-`.** Tab-complete typing speed. The `m` is
  short enough to live with on every checkout.
- **No agent prefix.** Author attribution lives in commit metadata
  (`git log --format='%an'`). The branch name describes the *work*,
  not the worker, and stays portable across agents (claude /
  opencode / human).
- **`chore/` and `hotfix/` outside the milestone scope.** Both step
  outside any specific release on purpose — they don't get a
  milestone prefix.
- **Multi-ticket form.** When a PR genuinely closes more than one
  ticket, join the numbers with `-` and pick a slug that names the
  *theme*, not any one ticket (e.g. `close-sweep`, not the slug of
  the first ticket alone).

### Bot-generated names

The cloud Claude Code session generates a working branch name from
the prompt (e.g. `claude/fix-init-defaults-NJybM`). Treat that as
**throwaway**. Before the first `git push` of the session, rename
to the canonical form:

```bash
git branch -m m2.5.0/feat-199-source-builder-removal
```

Then `git push -u origin <new-name>` as normal. The cloud-generated
branch never reaches the remote.

If the session goal is unclear at start (the user said "explore the
codebase"), defer the rename until the work crystallises into a
ticket or chore. When in doubt, branch as `chore/<rough-slug>` and
rename later if it becomes ticket-bound.

### What if there's no current milestone?

If `.repo/project/issues/` has no `MILESTONE-*.md` file (between
milestones, or the project hasn't reached its first one), use
`chore/<slug>` for everything. The next milestone-plan PR will
re-establish the prefix going forward.

## Commit subject

Every commit must reference exactly one issue. No commit without a ticket.

```
<TICKET-ID> - <title> - <Verb>
```

- **TICKET-ID** — `FEAT-NNN`, `BUG-NNN`, `DISC-NNN`, or `AUDT-NNN`
- **title** — the ticket's own title (or a shortened form of it)
- **Verb** — present-tense word describing what this commit does

Common verbs: `Created`, `Implemented`, `Fixed`, `Tested`, `Refactored`,
`Documented`, `Closed`, `Promoted`

Examples:
```
FEAT-037 - Workshop command via dwarf supervision - Implemented
FEAT-037 - Workshop command via dwarf supervision - Tested
BUG-008 - Fix bech32 encoding in bitcoin bin - Fixed
DISC-009 - Two interaction modes and workshop estimation - Created
AUDT-001 - Milestone 0.6.0 audit - Closed
```

Multi-commit sequences for one ticket (the common case) reuse the
same `TICKET-ID - title` prefix and vary only the verb. This makes
`git log --oneline` show the full story of a ticket's development.

**One issue per commit.** If a commit touches two unrelated tickets
it must be split. If a single logical change spans two tickets
(genuinely inseparable), use the primary ticket and note the secondary
in the commit body.

`chore/` and `hotfix/` branches are the only exception — they may use
a free-form subject when no ticket applies:

```
chore - Expand .gitignore for editor artifacts
hotfix - Rotate leaked API key
```

## Commit body

Free-form, but a useful pattern:

```
<one-paragraph summary of what + why>

Files changed:
- <key path>: <one-liner>
- ...

Verification:
- make lint → ok
- make check-unit → N tests
- ...

(Optional) Closes <TICKET-ID>.

Session: <agent>:<session-id>
https://claude.ai/code/session_<id>
```

## Session trailer

Commit messages end with a `Session:` trailer identifying the agent
session that produced the commit:

```
Session: claude-code:01ABC...
Session: opencode:xyz...
```

Supported agent prefixes:

| Prefix | Transcript store |
|---|---|
| `claude-code` | `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl` |
| `opencode` | `~/.local/share/opencode/project/<hash>/storage/session/info/<id>.json` |

`project effort` walks commits referencing a ticket, extracts the
`Session:` trailers, and dispatches to `project session-cost` per
agent to total tokens.

The `https://claude.ai/code/session_<id>` URL is the human-readable
counterpart for Claude Code; opencode does not have an equivalent
public URL. Both fields are optional individually; at least one
should appear in the trailer block for sessions you want attributed.

## PR title

Same as commit subject for single-commit PRs.

## PR body

Three sections, in order:

```
## Summary
<2-5 bullets: what changed, why>

## Verification
- make lint → ok
- make check-unit → N tests
- make -C <pkg> check → N tests
- make check-sit → ok / soft-skipped (no podman)
- ...

## Test plan
- [ ] CI: make lint clean
- [ ] CI: make check-all green
- [ ] (manual) <feature-specific>
```

PRs are opened **ready** (not draft) when local gates pass. Draft
state is reserved for explicitly-WIP changes the agent wants
review on before continuing.

## Retroactive renames

Merged branches don't show up in `git branch -a` once they're
cleaned up post-merge. Renaming history-only branches costs effort
for no downstream value. **Don't retroactively rename**; let old
names age out of the active list naturally. Only the *current*
worktable should follow the convention.
