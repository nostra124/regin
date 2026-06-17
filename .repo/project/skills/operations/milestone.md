---
name: milestones
description: |
  How we organise work into shippable milestones via
  per-milestone plan files (`issues/MILESTONE-<x>.<y>.<z>.md`).
  Trigger when scoping a new milestone, when shuffling
  features between milestones, when starting
  implementation on an open milestone, or when closing
  one out.
---

# `milestones` skill

## 1. The model

Work is delivered in **milestones**, each tied to a
specific future semver target. The unit of planning
is the **per-milestone plan files**:

```
issues/MILESTONE-<x>.<y>.<z>.md
```

One file per planned release version. The file is the
**central place** that lists every ticket bound for
that version, the rationale, the dependency order,
and the exit criteria.

Tickets themselves live where they always do:

```
issues/feature/<NNN>-<slug>.md     # open feature
issues/feature/done/<NNN>-<slug>.md # completed feature
issues/bug/<NNN>-<slug>.md         # open bug
issues/bug/done/<NNN>-<slug>.md    # completed bug
```

The milestone-plan files is the **assignment**; the ticket
file is the **specification**. Both must exist for an
in-flight feature.

### Early-milestone shape and UAT (RULE-014)

For a **fresh project** the first two milestones are fixed:

- **0.1.0 — roadmap and design.** Derive the roadmap (the ordered milestone
  plan) and resolve the design discussions (DISC tickets). No product code; the
  deliverable is the plan everything else is measured against.
- **0.2.0 — first deployable version.** A complete thin slice that installs:
  native packages, install path, `contrib/deploy`, and baseline docs — not a
  feature spike. From 0.2.0 on, there is always something a user can install.

From then on **every milestone ends shippable and UAT-able**: when it reaches
`stable`, the packaged artefact is handed to UAT, and UAT defects are filed as
BUG tickets in **patch milestones** (`0.x.y`) off it — never by reopening the
closed milestone.

**Reconcile every UAT finding with the roadmap.** Before a finding becomes a
bug, check it against the roadmap. If it restates work already planned for a
later milestone, decide *with the user* whether to raise a **partial** bug for
the part that is genuinely broken now, or defer to the planned milestone — and
record that decision on the finding. Do not silently file a full bug for
already-planned work, and do not silently drop it.

## 2. The development loop

```
1. Plan         →  assign tickets to a MILESTONE-<x>.<y>.<z>.md
2. Scope        →  one milestone at a time
3. Open branch  →  <agent>/roadmap-<x>.<y>.<z> off master (§2.2)
4. Implement    →  one session per milestone; phase PRs target the
                   integration branch
5. Close        →  merge integration branch → master; delete
                   milestone-plan files + branch; tickets stay in done/
```

The "**one milestone per session**" rule keeps the
work coherent: every PR in that session contributes to
the same release target, the integration branch is the
session's staging area, and the milestone-plan files is its
checklist.

### 2.1 Strict semver order

Milestones are implemented in **ascending semver
order**. With M2.1.0, M2.2.0, and M2.3.0 all open,
M2.1.0 ships first, M2.2.0 second, M2.3.0 third —
even if their `depends_on` chains say they're
independent. No jumping. No parallel milestones.

The unit of reordering is the **ticket**, not the
milestone. If the priorities shift, **move tickets
between milestone-plan files** (§4) rather than starting a
later milestone before an earlier one closes. The
semver number is a promise to the consumer about
release order; honour it.

Concretely: do not open a feature-implementation PR
whose target ticket lives in `MILESTONE-A.B.C.md` while
any `MILESTONE-X.Y.Z.md` with `(X.Y.Z) < (A.B.C)` is
still open on `master`. Spec-only PRs (ticket files,
milestone-plan files) are not subject to this rule — those
are planning artefacts, not implementation.

### 2.2 Milestone integration branch

Every milestone has a dedicated long-lived integration
branch off `master`, named:

```
<agent>/roadmap-<x>.<y>.<z>
```

For example: `claude/roadmap-2.1.0`,
`claude/roadmap-2.2.0`. `<agent>` is the agent (or
human) handle that opened the milestone; multiple
agents pair on one milestone via that branch.

**The flow:**

```
1. Open milestone  →  git checkout -b <agent>/roadmap-<x>.<y>.<z> master
2. Phase work       →  PRs target the integration branch, NOT master
3. Phase merge      →  PR auto-merges into the integration branch on
                       green CI (per operations/automerging.md)
4. Milestone close  →  When every ticket is done AND the exit criteria
                       pass on the integration branch, merge the
                       integration branch into master as a single
                       merge (squash or merge-commit per package
                       convention). Delete the integration branch.
```

**Why an integration branch:**

- `master` only sees milestone-complete merges — its
  history reads as a sequence of shipped versions,
  not a phase-by-phase scrub.
- Phase PRs can stack against the integration branch
  without leaking partial-milestone state to `master`.
- Rolling back a milestone is one revert.
- The branch name itself signals "milestone X.Y.Z is
  in flight" at a glance — no need to cross-reference
  the milestone-plan files.

**Branch naming:**

- `roadmap-` prefix (not `MILESTONE-`). The filename
  convention is `MILESTONE-<x>.<y>.<z>.md` for the
  planning artefact (§3); the **branch** uses
  `roadmap-` to keep the two namespaces visually
  distinct.
- Hyphens, not dots, after the prefix? **No.** The
  semver `<x>.<y>.<z>` keeps its dots:
  `claude/roadmap-2.1.0` (not `2-1-0`).

**Lifecycle invariants:**

1. The integration branch is created at milestone open
   and deleted at milestone close. It does not outlive
   the milestone.
2. Phase PRs **must** target the integration branch.
   A phase PR opened against `master` is a process
   violation — close it and re-open against the right
   target.
3. CI runs on every phase PR against the integration
   branch's HEAD; the **same** CI runs on the final
   integration-branch → master merge as the
   acceptance gate.
4. `master` never carries a milestone-in-flight
   commit. If you see a branch named `roadmap-<x.y.z>`
   merged into `master` mid-milestone, that is a
   mistake to roll back.
5. Branch naming is binding even for solo work; the
   integration-branch model is what makes
   collaboration possible without ad-hoc
   coordination.

**Rules-only and bug-fix PRs:**

PRs that don't touch milestone implementation (rules
clarifications, urgent bug fixes against `master`,
documentation tweaks) target `master` directly with
their own branch name (`<agent>/<topic>-<slug>`). The
integration-branch rule applies only to **milestone
implementation work** — code or tests that map back
to a ticket assigned to the current milestone.

### 2.3 Per-phase multi-ticket session rules

Whether a session may touch multiple tickets depends
on **which phase** the session is in:

| Phase | Multi-ticket scope | Notes |
|---|---|---|
| **Design (planning)** | **Cross-milestone allowed** | Discusses candidate tickets, assigns them to MILESTONE plan files, fills frontmatter (`milestone`, `complexity`, estimates). Output: updated MILESTONE plan files + per-ticket coarse design. |
| **Design (focused)** | Same-milestone only | Deep design for a specific ticket's approach — fills the ticket's `## Design` section. |
| **Build** | Same-milestone only | Starts with all bugs (priority order), then features (priority order) for the in-flight milestone. See `issues/feature/design.md` §3. |
| **Test** | Same-milestone only | SIT then PIT for the milestone's deliverables. |

**Forcing function:** if the in-flight milestone has
no remaining open tickets, you **cannot start a Build
session** — the next session must be a planning Design
session to populate the next milestone's plan. This
makes the milestone boundary a hard checkpoint.

**Token attribution per session type:**

| Session type | Tokens logged in | Rationale |
|---|---|---|
| Planning Design | `issues/MILESTONE-<ver>.sessions.jsonl` (milestone overhead) | Planning is shared overhead, not per-ticket work |
| Focused Design | Per-ticket `<id>.sessions.jsonl` | Single ticket = no split |
| Build (multi-ticket) | Per-ticket `<id>.sessions.jsonl`, **split at session end** by the agent | Agent records a `split_factor` per ticket; calibrate later |
| Test (multi-ticket) | Same as Build | |

The agent's judgement caps how many tickets fit in
one Build/Test session — no hard mechanical cap;
context limits self-enforce.

The `.sessions.jsonl` schema lives in
`.repo/project/skills/convention/tickets.md` →
"Session log".

## 3. Milestone-plan file shape

```markdown
---
milestone: <x>.<y>.<z>
title: <short milestone name>
status: active              # or: done when milestone is closed
depends_on: <prior milestone or ~>
---

# Milestone <x>.<y>.<z> — <name>

<one or two paragraphs: what this slot groups, why now>

## Issues

| ID       | Title                                 | Priority |
|----------|---------------------------------------|----------|
| BUG-NNN  | ...                                   | high     |
| FEAT-MMM | ...                                   | medium   |

## Delivery prerequisites (required before alpha can start)

A milestone plan is **incomplete** without a ticket for each of the four
delivery prerequisites below.  These are not alpha exit criteria — they are
planning prerequisites.  File the tickets here at kickoff; do not begin any
implementation work until the table is fully populated.

| Prerequisite | Ticket | Status |
|---|---|---|
| 100% test coverage | FEAT-NNN | open |
| Native packages, all platforms + GitHub release | FEAT-NNN | open |
| Install script (PIT-tested) | FEAT-NNN | open |
| GitHub wiki landing page | FEAT-NNN | open |
| Mobile app (if defined by project) | FEAT-NNN or N/A | open/n/a |

This table is audited during the `open → alpha` gate check (RULE-010).

> **"Native packages" means real package formats, not just a binary tarball.**
> Every supported platform in `profile.md` §7 must have its native package
> (`.deb`, `.rpm`, `.apk`, macOS `.pkg`, Homebrew, …), and every package must
> ship **all** of the milestone's binaries plus their service units — not only
> the primary CLI. A tarball-only pipeline does **not** satisfy this
> prerequisite (this is the gap that produced BUG-002/BUG-003 in 0.9.0). If the
> milestone adds a new binary or daemon, updating the packages is part of that
> binary's ticket, not a follow-up.

## Suggested delivery order

<dependency-aware order; e.g. "FEAT-A before FEAT-B
because B's tests depend on A's helpers">

## Effort estimate

| Ticket   | Size | Tokens   | Time      |
|----------|------|----------|-----------|
| BUG-NNN  | S    | 10-30k   | 15-30min  |
| FEAT-MMM | M    | 30-80k   | 30-90min  |
| **Total**|      | **~X-Yk**| **~A-Bhr** |

(The sizing rubric anchoring these ranges lives in
`issues/feature/design.md` §2.)

## Exit criteria

- <verifiable condition for milestone closure>
- ...
```

The Effort estimate table sums each ticket's
`complexity` / `estimate_tokens` / `estimate_time`
frontmatter fields. The totals are forecasts, not
commitments — `project effort milestone <ver>`
(FEAT-192) reports actuals once sessions accumulate.

Alongside the MILESTONE plan file, planning Design sessions
write to **`issues/MILESTONE-<ver>.sessions.jsonl`**.
That file captures the planning overhead — tokens
spent on milestone scoping that isn't attributable to
any single ticket. Schema in
`.repo/project/skills/convention/tickets.md` →
"Session log".

## 4. Shuffling tickets between milestones

Tickets are fungible across milestones; milestones
themselves are not reorderable (§2.1). When a
priority shift means "X needs to ship sooner," the
answer is to **move X's ticket into an earlier
milestone**, not to start a later milestone out of
order.

When a ticket moves from one milestone to another:

1. **Remove** the row from the source milestone-plan table.
2. **Add** the row to the target milestone-plan table.
3. **Re-check** the exit criteria of both files — does
   the source still hold without that ticket? Does
   the target's order still make sense with it
   added?
4. **Update** the `depends_on` chain if the shuffle
   creates or breaks an ordering dependency.
5. **Commit** as one atomic change: "milestone-plan: move
   FEAT-NNN from X.Y.Z to A.B.C — <reason>".

The ticket file itself does **not** move — it stays
under `issues/feature/<NNN>-*.md`. Only its milestone plan
assignment changes.

## 5. Milestone phase promotion (open → alpha → beta → stable)

A milestone moves through four phases.  Full criteria are in
`.repo/project/audit/rules/RULE-010-phase-entry-criteria.md`.

| Phase | Who acts | Gate |
|---|---|---|
| **open** | Planners | Milestone plan complete with all four delivery prerequisites ticketed (see below) |
| **alpha** | Single internal user | All FEATs done; tests 100%; packages, install script, wiki delivered; audit filed |
| **beta** | Multiple users | Alpha signed off; alpha-feedback BUGs resolved |
| **stable** | General availability | Beta signed off; git tag `v<version>` exists |

### Alpha and beta: package and deploy every fix for UAT

Alpha and beta are **user-acceptance** phases — the tester exercises the real
packaged artefact, not source. So during alpha and beta, **every** fix follows
the loop:

1. Implement the fix and auto-merge it once CI is green.
2. **Produce the package fixes** — rebuild the native packages
   (`make packages` / `packages/build-all.sh`) so the merged fix is in a real
   `.deb`/`.rpm`/`.apk`/`.pkg`, not just on `master`.
3. **Release / deploy for UAT where possible** — roll the rebuilt packages out
   to the UAT host(s) with `contrib/deploy` (see the `deploy` skill) so the
   tester runs the packaged fix. If a host is unreachable or the arch cannot be
   built locally (see the qemu/CI note in `packages/README.md`), say so and fall
   back to the CI-built packages on the pre-release.

This is not optional polish: a fix that is merged but not packaged-and-deployed
has not actually reached the person doing acceptance testing. The
milestone-cycle workflow (BUG-005/006) carries these as steps so they run every
iteration, not just at milestone end.

**Install only from the native package — never from source.** Throughout alpha
and beta, every install (the tester's, and any local one) is done by installing
the built native package, not `cargo install` / `make install` / a hand-copied
binary:

```
sh packages/macos/build-pkg.sh packages/dist          # or make packages
sudo installer -pkg packages/dist/dvalin-<ver>-macos-<arch>.pkg -target /   # macOS
sudo dpkg -i  dvalin_<ver>_<arch>.deb   # Debian/Ubuntu
sudo rpm  -i  dvalin-<ver>.<arch>.rpm   # Fedora/RHEL
sudo apk add --allow-untrusted dvalin-<ver>-r0.<arch>.apk   # Alpine
```

A binary that wasn't installed from the package was not acceptance-tested —
package metadata, service units, file layout, and post-install steps are part
of what UAT validates.

### Milestone demo: run it from the installed package

At the **end of each milestone**, before sign-off, do a demo — and run the demo
from the **installed native package**, not from `target/release` or `cargo run`.
Build the package, install it (commands above), and drive the milestone's exit
criteria through the installed binaries. If the demo cannot run from the package,
the milestone is not actually shippable and is not done, regardless of what the
tests say. This is the concrete proof of RULE-014's "every milestone ends
shippable and UAT-able".

### Before alpha can start — four delivery prerequisites

These are **planning requirements**, not implementation requirements.  Before
a milestone transitions from `open` to `alpha`, each of the following must
have a FEAT ticket in the milestone's `tickets:` list.  A milestone plan
without them is incomplete and must not begin implementation work.

| Delivery prerequisite | Must have a ticket for… |
|---|---|
| **100% test coverage** | committing to full unit + SIT + PIT coverage for all modules in this milestone |
| **Platform packages** | building + uploading **native packages** (`.deb`, `.rpm`, `.apk`, macOS `.pkg`, Homebrew — per `profile.md` §7) for every supported platform to the GitHub release, each shipping **all** the milestone's binaries + service units; a binary tarball alone does not satisfy this. Include mobile targets if the project defines them |
| **Install script** | `install.sh` (or equivalent), PIT-tested end-to-end |
| **GitHub wiki** | landing page: description, install instructions, quick-start; current with this milestone |

File these tickets at **milestone kickoff** (`dvalin dev milestone kickoff` step 4).
If any are missing when kickoff runs, file them before writing any code.

### Alpha promotion checklist (all must be done)

1. Every FEAT ticket in `tickets:` is in `done/` — including the four above.
2. Test coverage is **100%** across all modules.
3. Unit, SIT, and PIT all green.
4. No open design questions (RULE-005); no stubs (RULE-006); no open BUGs.
5. Native packages on GitHub for all supported platforms — `.deb`/`.rpm`/`.apk`/
   macOS `.pkg`/Homebrew, each shipping every binary + service units (not just a
   tarball); + mobile if defined.
6. Install script exists and PIT exercises it.
7. GitHub wiki landing page current with this milestone's features.
8. AUDT ticket in `audit/done/`.

**Promoting with dvalin:**

```
dvalin dev release --promote   # alpha → beta  or  beta → stable
```

This command verifies all entry criteria before bumping the version
suffix in `Cargo.toml` (and `configure.ac` if present) and updating the
milestone frontmatter `phase:` field.

**Sign-off** is recorded as a comment or closing note directly in the
milestone file, e.g.:

```markdown
## Alpha sign-off

Accepted by <user> on <date>. Known minor issues filed as BUG-042, BUG-043
(target: 0.6.1 patch milestone).
```

Skipping a phase is a block violation (RULE-010) unless the milestone
file carries `single-phase: true` and the project owner has explicitly
approved it there.

## 6. Closing a milestone

Once every ticket on the integration branch has merged
in (per §2.2) and the exit criteria are green:

1. **Verify** every ticket in the table has a
   matching file under `issues/<type>/done/` (move
   happens on the integration branch as each phase
   PR lands).
2. **Confirm** the exit criteria in the milestone-plan files
   are satisfied — run the full test matrix
   (`make check` + `make check-sit` + `make check-pit`
   where applicable) on the integration branch's
   HEAD. All green is the precondition for the
   master merge.
3. **Flip** the frontmatter `status: active` → `status: done`
   in the same commit that closes the last ticket
   (optional but useful for audit trails).
4. **Delete** the milestone-plan files **and its
   `MILESTONE-<x>.<y>.<z>.sessions.jsonl` sibling**.
   Both are planning artefacts; their history lives
   in git. Per-ticket `<id>.sessions.jsonl` files are
   different — those follow their ticket to `done/`.
5. **Merge** the integration branch into `master`
   (§2.2). The merge commit's title is the milestone
   name; its body summarises the shipped tickets.
6. **Delete** the integration branch (`git branch -d
   <agent>/roadmap-<x>.<y>.<z>` + remote prune). It
   does not outlive the milestone.

The file is removed from `master`; the commit history
preserves every prior state. `git log
issues/MILESTONE-<x>.<y>.<z>.md` reconstructs the full
lifecycle whenever needed. **No archive directory**
for milestone plans — they live in git history, full stop.

Completed **tickets** (features and bugs) are
**moved** to `issues/<type>/done/`, never deleted:
they are the traceability artefacts that map
functionality back to a specification. See
`operations/audit.md` for how that traceability is
verified.

## 7. Why delete the milestone-plan files but keep tickets?

| Artefact                          | Lifecycle    | Why                                                    |
|-----------------------------------|--------------|--------------------------------------------------------|
| Milestone plan file                      | deleted on close | A plan. Once executed, it's history; keep it in git. |
| `MILESTONE-<ver>.sessions.jsonl`    | deleted on close | Planning-overhead log. Same lifecycle as the milestone plan itself. |
| Feature ticket                    | moved to `done/` | A spec. Maps functionality ↔ rationale; preserved.   |
| Bug ticket                        | moved to `done/` | The regression record; preserved.                    |
| Per-ticket `<id>.sessions.jsonl`  | moves with the ticket to `done/` | Provenance for effort spent; preserved alongside the spec. |

The milestone-plan files (+ its sessions.jsonl) is *planning*;
tickets (+ their sessions.jsonl) are *provenance*.
Different lifecycles for different purposes.

## 8. Naming + version routing

Per `.repo/project/skills/methodology/vmodel.md` →
"Milestone planning":

| Change kind             | Bump  | Filename pattern        |
|-------------------------|-------|-------------------------|
| Bug fix                 | patch | `MILESTONE-X.Y.<patch>.md` |
| Additive feature        | minor | `MILESTONE-X.(Y+1).0.md`   |
| Breaking surface change | major | `MILESTONE-(X+1).0.0.md`   |

### Patch milestones are planned before implementation (RULE-015)

A patch (`0.x.y`, `z > 0`) follows the same plan-before-build gate as a feature
milestone. Before any patch code is written:

1. Write the `kind: patch` milestone file with a goal, the **ordered** bug list,
   and exit criteria; point every bug's `milestone:` at it.
2. **Multiple bugs may share one patch version** — group related fixes into one
   `0.x.y` rather than minting a version per bug. But each bug is **one commit**
   (`BUG-NNN - <title> - Implemented`); never bundle bugs in a commit, never
   spread one bug across "implemented" commits.
3. Bump `Cargo.toml` to the patch version **once**, at cut time
   (`dvalin dev release --patch`) — not per bug, not mid-implementation.

The `MILESTONE-` prefix is canonical for the **filename**;
the **branch** uses the `roadmap-` prefix per §2.2.
Two namespaces, deliberately distinct:

| Artefact         | Prefix     | Lives in       |
|------------------|------------|----------------|
| Planning file    | `MILESTONE-` | repo content   |
| Integration branch | `roadmap-` | git ref namespace |

Earlier in this codebase you may find `ROADMAP-`
(capitalised) as a file prefix — that was a deprecated
naming, renamed to `MILESTONE-` on sight (content
preserved).

## 9. The unassigned pool: open tickets in `issues/`

There is no `BACKLOG.md`. The unassigned pool is simply every ticket
file that exists in `issues/feature/` or `issues/bug/` but does not
appear in any open milestone's `tickets:` list.

Lifecycle:

1. A DISC discussion concludes → a FEAT ticket is filed in
   `issues/feature/`. It is unassigned until milestone planning picks
   it up.
2. Milestone planning selects tickets → their IDs are added to
   `MILESTONE-<ver>.md` under `tickets:`.
3. The milestone closes → the milestone file gets `status: done`; the
   ticket itself moves to `done/`. Nothing re-enters the unassigned
   pool from a merged ticket.
4. If a milestone is cancelled mid-flight or a ticket is dropped from
   a plan, remove it from the milestone's `tickets:` list. The ticket
   file remains open in `issues/feature/` for the next planning cycle.

New feature ideas must always originate from a DISC ticket (see
`methodology/discovery.md`). Filing a FEAT directly without a DISC is
only permitted for bugs — those may be filed from observed failures
without a prior discovery session.

An audit (`operations/audit.md`) catches tickets that exist on disk but
appear in no milestone (unassigned drift) or appear in more than one
milestone (double-assignment).

## 10. Guardrails

1. **Never delete a ticket file** when closing a
   milestone. Move to `done/`.
2. **Never modify a ticket file** when shuffling
   between milestones. Only the milestone-plan row moves.
3. **Never split a milestone mid-flight** without
   updating the milestone-plan files *in the same commit*
   as the split decision.
4. **Never start implementation** on a milestone
   whose milestone-plan files has `status: active` *and*
   unresolved dependency placeholders (`depends_on`
   pointing at an open milestone plan). Close the
   dependency first.
5. **Multi-ticket session scope follows §2.3** — Build
   and Test sessions are same-milestone only; planning
   Design sessions may span milestones; focused
   Design / Build / Test never interleave work from
   two different milestones. The cross-cutting
   context wastes attention.
6. **Never start a higher-versioned milestone
   before a lower-versioned one is closed** (§2.1).
   The semver order is binding. Re-route via ticket
   moves (§4), not by jumping milestones.
7. **Never open a phase PR against `master`** when
   the milestone has an integration branch (§2.2).
   The PR's base must be `<agent>/roadmap-<x>.<y>.<z>`.
   Rules-only and bug-fix PRs against `master` are
   exempt — they don't carry milestone implementation.
8. **Never merge an integration branch into `master`
   before the milestone exit criteria are green**
   (§5). The branch is the staging area; `master`
   sees only milestone-complete merges.

## 11. Worked example

This codebase's recent history is the worked example:

- M1.0.1 + M1.1.0 — shipped via PR #1, then the two
  milestone-plan files (`ROADMAP-1.0.1.md`,
  `ROADMAP-1.1.0.md`) were deleted. Tickets moved to
  `issues/{bug,feature}/done/`.
- M2.0.0 + M2.1.0 — pre-§2.2 milestones; phase PRs
  landed straight on `master`. Grandfathered.
- M2.2.0 onwards — first milestones under the
  integration-branch rule. Open
  `claude/roadmap-2.2.0` off the master tip that
  follows M2.1.0's close, target every phase PR at
  that branch, merge to master only when exit
  criteria are green.

## 11b. Supervised execution

For multi-ticket milestones, drive the loop end-to-end in **one
reused session** via **`project supervise <milestone>`** (FEAT-187).

The verb writes a session-scoped state file and prints the Claude
Code `settings.local.json` fragment that wires its companion **Stop
hook** (`libexec/project/supervise-stop-hook`) into the running
session. The hook fires whenever the agent ends a turn and decides
one of three actions:

| Repo state                                                   | Action       |
|--------------------------------------------------------------|--------------|
| MILESTONE plan file gone / empty                                    | `close`      |
| Every listed ticket already in `done/`                       | `close`      |
| Current ticket's PR merged + ticket in `done/` + clean tree  | `checkpoint` |
| Anything else with work in flight                            | `continue`   |

- **continue** — refuse the stop; inject a re-grounding payload so
  compaction can't lose the milestone context. Branch, working-tree
  state, PR state, and next concrete step are reflected on every
  fire.
- **checkpoint** — allow the stop at a ticket boundary. The agent
  pauses; the user types `project supervise resume` to continue, or
  `project supervise --unregister` to bail.
- **close** — allow the stop. Milestone is complete (or the close PR
  is the only remaining step); the audit sweep
  (`operations/audit.md`) runs as the final act.

CI waits are unchanged: the standard `features.md` flow calls
`subscribe_pr_activity` after pushing; the session sleeps; the
webhook wakes it. The Stop hook does **not** fire during sleep —
only when the agent explicitly tries to end the turn. No polling.

State introspection without invoking the LLM:

```
project supervise --status
```

prints the milestone, plan-file path + ticket counts, last hook
action, and the state file location.

This is the recommended way to drive a milestone — preserves
context across tickets, mechanises the cleanup checklist, and
prevents premature stops.

## 12. Cross-references

- Bug-handling protocol (TDD): `issues/bug/discovery.md`
- Feature delivery walkthrough: `issues/feature/design.md`
- Auto-merging on close: `operations/automerging.md`
- Traceability audit: `operations/audit.md`
- Supervised execution: §10b above + FEAT-187
  (`issues/feature/done/187-project-supervise.md`)
- Binding rule on bug-before-feature ordering:
  `.repo/project/skills/methodology/vmodel.md` →
  "Bug ↔ feature ↔ semver routing"
- Convention on milestone-plan file shape:
  `.repo/project/skills/convention/tickets.md` →
  "MILESTONE-`<x>.<y>.<z>.md` file"
- Phase entry criteria (alpha/beta/stable):
  `.repo/project/audit/rules/RULE-010-phase-entry-criteria.md`
