# Ticket + milestone + session conventions

> Frontmatter schema, file naming, session log format, and milestone
> conventions. Compare with `policy/transitions.md` (binding gates)
> and `methodology/vmodel.md` (phase model).

## Feature / bug ticket

Frontmatter (YAML, on top of the markdown file):

```
---
id: FEAT-NNN              # or BUG-NNN
type: feature             # or bug
priority: low|medium|high|critical
complexity: XS|S|M|L|XL    # T-shirt size; see issues/feature/design.md §2
estimate_tokens: 30k-80k   # range; set in Design phase (features) or at filing (bugs)
estimate_time: 30-90min    # range; LLM wall-time, excluding CI + user
phase: open                # open | design | build | test | done
                            # bugs skip 'design' — see skills/bugs.md §1
status: open               # or done (when moved to feature/done/)
depends_on: FEAT-NNN       # optional; references another ticket
---
```

File naming:
- features: `<NNN>-<kebab-slug>.md` under `issues/feature/<phase>/`
- bugs: `<NNN>-<kebab-slug>.md` under `issues/bug/<phase>/`
- closed: same path with `done/` replacing the phase directory

Phase directories:
- features: `open/`, `design/`, `build/`, `test/`, `done/`
- bugs: `open/`, `build/`, `test/`, `done/` (bugs skip `design/`)
- discovery: **flat** — `DISC-NNN.md` directly under `discovery/`, moved to
  `discovery/done/` when closed (no phase subdirs; BUG-053)

A ticket is filed into its initial phase directory and moved
forward by `project transition <id> <phase>`. The directory
always matches the `phase:` frontmatter field.

Sections:

```
# <Title>

## Description
**As a** <role>
**I want** <thing>
**So that** <outcome>

<one-or-two-paragraph context>

## Implementation
<implementation notes; can be sparse — this is a planning doc>

## Acceptance Criteria
1. <verifiable criterion>
2. ...
```

When the ticket is closed, append a Resolution section:

```
## Resolution

Closed by the FEAT-NNN finishing PR (link).

Acceptance check:
1. ✅ <criterion>
2. ✅ <criterion>
3. ❌ **Divergence:** <what spec said vs what shipped, and why>
4. ⚠️ <partial; deferred to FEAT-MMM follow-up>
```

The `✅ / ❌ / ⚠️` markers are intentional — they make the
divergence audit fast.

## Discovery ticket

Discovery tickets are the upstream artefact for problem framing
+ solution ideation. Different file location, different phase enum,
otherwise the same shape as a FEAT.

Frontmatter:

```
---
id: DISC-NNN              # independent numbering from FEAT/BUG
type: discovery
priority: low|medium|high|critical
status: open              # or done (when moved to discovery/done/)
complexity: XS|S|M|L      # T-shirt size of expected delivery
spawned_features: ~       # or a list like [FEAT-201, FEAT-202]
---
```

A spawned FEAT carries the reciprocal field:

```
---
id: FEAT-NNN
...
spawned_from: DISC-NNN    # optional; set by `project discover --link`
---
```

File naming:

- active: `DISC-<NNN>-<kebab-slug>.md` directly under `issues/discovery/`
- closed: same file moved to `issues/discovery/done/`

Sections (canonical scaffold from `project discover <topic>`):

```
# DISC-NNN — <topic>

## Describe

<problem statement, evidence, audit results, user impact>

## Variants considered

<list every option that was on the table, even rejected ones>

| Variant | Summary | Key trade-off |
|---|---|---|
| A | ... | ... |
| B | ... | ... |

## Decision matrix

| Criterion | Weight | Variant A | Variant B |
|---|---|---|---|
| <criterion> | high/med/low | ✓/✗/~ | ✓/✗/~ |

## Arguments

### Pro (chosen approach)

- ...

### Con / risks

- ...

## Decision

**Chosen:** Variant X

**Why:** <one-paragraph rationale — not just what was decided but why
this variant over the others, which constraints were decisive, and
what trade-offs were consciously accepted>

## Spawned features

(record each minted FEAT here)
```

**Capturing the why is mandatory.** A DISC ticket that records only the
decision without variants, matrix, and rationale is incomplete. The
dwarf facilitating the workshop is responsible for writing all sections
before marking the DISC `status: done`.

A DISC is moved to `discovery/done/` (`status: done`) only once it records the
**decision with rationale** (the captured "why" — variants, what was decisive,
trade-offs accepted) and any `spawned_features`. There are no intermediate
`describe`/`ideate` phase directories; discovery is flat (BUG-053).

Full protocol in `issues/discovery.md`.

## MILESTONE-`<x>.<y>.<z>.md` file

Per-version milestone-plan files live under
`.repo/project/issues/`:

```
.repo/project/issues/MILESTONE-0.18.5.md
.repo/project/issues/MILESTONE-0.18.5.5.md   (bugs against 0.18.5)
.repo/project/issues/MILESTONE-0.19.0.md
```

A `.repo/project/issues/BACKLOG.md` is the **unassigned pool** —
tickets that exist on disk but haven't yet been bound to any
open `MILESTONE-<x>.<y>.<z>.md`. New work is filed straight into
`BACKLOG.md` (or appears there from a closed milestone that
deferred items); a ticket leaves the pool the moment a
`MILESTONE-` file lists it.

Each per-version file has:

```
# v<x>.<y>.<z> — <short milestone name>

<one-paragraph rationale: why this slot, what it groups>

## Tickets

| FEAT  | Title                              | Status |
|-------|-------------------------------------|--------|
| FEAT-NNN | <title>                          | open / in-flight / done |
| ...                                                          |
```

Status values:
- `open` — ticket exists, no work started
- `in-flight` — a PR is open against this ticket
- `done` — merged

The full lifecycle (shape, shuffle, close, audit hooks) is the
**`operations/milestone.md`** walkthrough.

When every ticket listed in a milestone-plan file has shipped,
the file is **deleted** in the same commit that closes the last
ticket; tickets stay in `issues/{feature,bug}/done/` for
traceability (see **`operations/audit.md`**).

## Session log

Each ticket and each in-flight MILESTONE plan file may have a
sibling `.sessions.jsonl` recording the agent sessions spent on it.
Append-only, one JSON object per line:

```
.repo/project/issues/feature/<NNN>.sessions.jsonl          # per-ticket log
.repo/project/issues/bug/<NNN>.sessions.jsonl              # per-ticket log
.repo/project/issues/MILESTONE-<x>.<y>.<z>.sessions.jsonl  # milestone-planning log
```

### Line schema

```json
{
  "phase": "design|build|test",
  "started": "2026-05-13T10:00:00Z",
  "ended": "2026-05-13T10:45:00Z",
  "agent": "claude-code|opencode",
  "session_id": "01ABC...",
  "tokens": {"input": 12000, "output": 6000, "cache_read": 0, "cache_create": 0},
  "split_factor": 1.0,
  "notes": "optional free-text"
}
```

Field semantics:

- **phase** — the V-model phase this session contributed to. Always
  one of `design` / `build` / `test`. Planning Design sessions in a
  `MILESTONE-<ver>.sessions.jsonl` use `phase: "design"` (the
  *planning* qualifier is implicit from the file location).
- **started / ended** — ISO-8601 UTC. The agent records these at
  phase boundaries (`project transition` does this).
- **agent + session_id** — what `project session-cost` dispatches
  on to read the transcript.
- **tokens** — totals from the transcript. May be `null` if the
  session pre-dates token tracking (e.g. an opencode session run
  without local transcript retention).
- **split_factor** — for multi-ticket Build/Test sessions, the
  fraction of the session attributed to *this* ticket. The same
  `session_id` appears in multiple ticket logs with split_factors
  summing to 1.0. For single-ticket sessions, `1.0`. Planning Design
  sessions never split (they live in the MILESTONE sessions.jsonl).
- **notes** — free-text. Optional. Useful for "context exhausted at
  ticket 3 of 5" annotations.

### Lifecycle

Per-ticket `.sessions.jsonl` files move with the ticket to `done/`
on merge — they are provenance and stay forever.
`MILESTONE-<ver>.sessions.jsonl` files are deleted alongside the
MILESTONE plan file when the milestone closes — they are planning
artefacts and history-in-git is enough.

See `operations/milestone.md` for the full attribution model and
lifecycle table.

## Session retro file

One file per session at `retro/YYYY-MM-DD-<slug>.md`.

```
retro/
  2026-05-15-rpk-author-2x-syntax.md
  2026-05-15-cross-project-boundary.md
```

Frontmatter + structured sections per `operations/retrospective.md`. The file
lives at the repo root (not under `issues/`) because it documents
a session, not a ticket — the session may span multiple tickets or
none.