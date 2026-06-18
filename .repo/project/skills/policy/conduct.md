# Conduct: boundaries, pause-triggers, never-autonomously list

> Hard stops and pause-triggers that override the autonomous loop.
> Cross-references `methodology/agent-loop.md` (the loop) and
> `policy/merging.md` (auto-merge conditions).

## Cross-project boundaries

The agent MUST NOT modify files outside the current project's working
tree — this includes the rpk staging area (`~/.local/src/<pkg>/`) and
any other repository under `~/Projekte/`.

**Hard-stops:**

| Situation | Correct action |
|---|---|
| A skill / rule / tool belongs to a different project (e.g. `rpk-author` is part of rpk, not the target project) | File the ticket in that project's repo under `~/Projekte/<project>/issues/<type>/<phase>/` |
| A build or test issue is caused by code in the staging area (`~/.local/src/<pkg>/`) | File a BUG in the corresponding `~/Projekte/<project>/issues/bug/open/` |
| A dependency from another rpk package needs updating | File a FEAT or BUG in that package's repo |
| The current project needs a feature that depends on changes to another project | File a FEAT in the other project, then reference it from this project's ticket |

The `~/Projekte/` directory is the single source of truth for all
active development repositories. The staging area under `~/.local/src/`
is a read-only build artifact managed by `rpk stage` and `rpk update`.

## Hard "never autonomously" list

The agent must NOT take these actions without explicit user
instruction in the same turn:

| Action | Why |
|---|---|
| `--no-verify` (skip git hooks) | hooks exist for reasons; skipping is a manual override |
| `--no-gpg-sign` (skip commit signing) | trust chain matters; skipping is a manual override |
| `--amend` of any pushed / shared commit | rewrites history; PR review tracking breaks |
| `git push --force` / `--force-with-lease` to a branch with PRs open | overwrites work others may have based on |
| `git push --force` to `master` (or any default branch) ever | end of trust |
| Push directly to `master` / default branch | bypasses review |
| `git reset --hard` of work that isn't already committed elsewhere | data loss |
| Delete a branch the agent didn't create | not its to delete |
| Close a ticket that isn't actually resolved by the merged PR | misrepresents state |
| Run destructive commands on production data (drop tables, rm -rf system paths, kill non-test processes) | catastrophic |
| `sleep` / poll / loop waiting for CI (or any external webhook event) | session hang; webhooks deliver the outcome — see `policy/merging.md` |
| Modify files outside the current project's working tree (including `~/.local/src/` staging area, other repos under `~/Projekte/`) | violates "Cross-project boundaries" policy; file a ticket instead |

Authorisation for these is **single-use** — a previous OK does not
authorise later runs. Each occurrence asks again.

## Pause-and-ask triggers

The agent stops the autonomous loop and asks the user when:

- Same CI failure has repeated 3 times (tracked via webhook events,
  not polling — root cause not converging).
- A PR review comment is ambiguous or architecturally significant.
- An exploratory user question arrives mid-PR-loop ("what do you
  think about X?") — answer first; resume only on explicit go.
- The agent realises mid-implementation that the ticket scope is
  wrong (the AC are unachievable as stated, or the implementation
  reveals a different correct shape).
- A change is needed in a file on the shared-infrastructure list.
- The agent finds an unfamiliar file or branch in the working tree
  that might represent the user's in-progress work — investigate,
  do not delete or overwrite.
- A change or ticket involves a different project (see "Cross-project
  boundaries") — file the ticket in that project's repo, do not
  modify its working tree here.

## Session retrospective

The agent MUST write a retro entry at `retro/YYYY-MM-DD-<slug>.md`
before ending any session that produced one or more of:

- A PR opened or merged
- A ticket filed (FEAT, BUG, or DISC)
- Code changes committed
- A discovery session with written findings

The format and process are defined in `operations/retrospective.md`. Cross-project
findings (e.g. a tool bug in another project) get a ticket filed in
that project's repo under `~/Projekte/<project>/issues/` first, then
referenced in the retro.

**Hard-stop** — a session that produced any of the above but ends
without a corresponding retro file is a policy violation. The only
exception is a purely exploratory session with no written artefacts.

## Issue phase directories

Tickets are filed under
`.repo/project/issues/<type>/<phase>/<NNN>-<slug>.md` and **must**
reside in the directory matching their `phase:` frontmatter field.
When `project transition <id> <phase>` advances a ticket, the
ticket file is moved to the new phase directory.

| Type | Phase directories | Initial phase |
|---|---|---|
| feature | `open/`, `design/`, `build/`, `test/`, `done/` | `open/` |
| bug | `open/`, `build/`, `test/`, `done/` | `open/` |
| discovery | `describe/`, `ideate/`, `done/` | `describe/` |

**Hard-stop** — a ticket whose file location does not match its
`phase:` frontmatter is a policy violation. The directory and the
frontmatter field MUST agree at all times.