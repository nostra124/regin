---
name: retro
description: |
  Session retrospective — capture lessons learned at
  the end of every autonomous session. Triggered by
  policy: the agent writes a retro entry before ending
  any session that produced a commit, opened a PR, or
  filed a ticket.
---

# `retro` skill

## 1. Goal

Close the learning loop. Every session surfaces something
unexpected — a tool quirk, a policy gap, a recurring
friction. If it's not written down, the next session
repeats the same mistake.

One retro file per session, stored at `retro/`.

## 2. When

After step 10 in the agent loop (review comments resolved,
PR merged or ticket filed), **before** the session ends.

If the session produced no artefacts (purely exploratory,
reading docs), the retro is optional — the agent skips it.

## 3. File naming

```
retro/YYYY-MM-DD-<slug>.md
```

`<slug>` is a short kebab-case description derived from the
session's main topic or the ticket IDs worked on. Examples:

```
retro/2026-05-15-rpk-author-2x-syntax.md
retro/2026-05-15-feat-199-cross-project-boundary.md
```

## 4. Format

Frontmatter (YAML):

```
---
date: 2026-05-15
session: <agent>:<session-id>
tickets:
  - FEAT-NNN
  - BUG-NNN
tokens:
  input: N
  output: N
outcome: success | partial | failed
---
```

Body (free-form markdown, structured sections):

```
# YYYY-MM-DD: <one-line summary>

## What surprised me

<unexpected tool behaviour, codebase discovery, process friction>

## Policy / convention gap

<any rule I should have had but didn't; any rule I had
that misled me. If a change is warranted, file a ticket
and link it here.>

## Filed tickets

- [FEAT-NNN](issues/feature/NNN-*.md): <title>
- [BUG-NNN](issues/bug/NNN-*.md): <title>

## Next time

<actionable change to my own process>
```

## 5. Enforcement

Writing the retro entry is a **binding policy** (see
`.repo/project/skills/policy/conduct.md` → "Session
retrospective"). The agent MUST write it before ending
the session when any of:

- A PR was opened or merged
- A ticket was filed
- Code changes were committed (including `.rpk/` changes)
- A discovery session produced findings

Skipping a retro for one of the above is a policy
violation — the same class as skipping a required test.

## 6. Ticket spawning

If the retro identifies a policy gap or recurring friction
that warrants a ticket, file it **immediately** (before
the retro file is written) so the retro can reference it
by ID. The ticket goes into the affected project's
`issues/` directory, not into the current project's
unless they are the same.

Cross-project findings (e.g. "rpk-author skill uses 1.x
syntax") become tickets in the affected project's repo
under `~/Projekte/<project>/issues/`.
