---
name: issue-discovery
description: |
  How we capture problem framing and solution ideation as durable
  artefacts before feature tickets are filed. The upstream
  dual-track companion to issue delivery. Trigger when a problem
  feels worth recording but it's too early to specify a FEAT —
  or when one FEAT is clearly only part of a bigger conversation.
---

# Issue discovery skill

## 1. What this is

The **discovery track** is the upstream phase that produces
FEATs, not the place we ship code. It captures *why* a piece of
work happened — the problem statement, the audit evidence, the
solutions considered, the FEATs ultimately minted — in a single
durable artefact: the `DISC-NNN` ticket.

Inspired by — but deliberately simpler than — design thinking.
Two working phases plus done; no Empathize / Prototype / Test
(those are either out of scope or duplicate the V-Model's Build /
Test phases).

```
  describe → ideate → done
     │         │        │
     │         │        └─ archived to issues/discovery/done/
     │         │           with spawned FEATs linked
     │         │
     │         └─── solution discussion; FEATs MINTED here
     │              (project discover --link <id> FEAT-N)
     │
     └─── problem statement, evidence, audit results, user impact
```

## 2. When to use it (and when not to)

**Use DISC** when:

- You see a problem worth recording but you don't know yet how
  many FEATs it'll take. (One DISC → 0…N FEATs.)
- You want the *WHY* preserved alongside the *WHAT*. Audit
  cares (`issues/bug/design.md` → audit interaction).
- A user / agent conversation surfaces tangled requirements
  that need framing before scoping.
- A bug investigation reveals a design issue (per
  `issues/bug/design.md` → "Design-issue escape hatch") — file a
  fresh DISC, not a FEAT, until the shape is clear.

**Don't use DISC** when:

- The problem is a single trivial bug → file a BUG.
- The feature is fully specified already → file a FEAT directly.
- You're just thinking out loud → don't manufacture process.

## 3. The dual track

Discovery and Delivery run in **parallel**, not in sequence:

```
  Discovery (continuous)             Delivery (milestones)
  ─────────────────────              ─────────────────────────
  describe  ┐                          MILESTONE-2.5.0
            │ ideate →                   FEAT-160 (build)
            │   (mints FEAT)   ────►     FEAT-159 (build)
            │ done                       FEAT-163 (build)
                                            │
                                            ▼
                                          (closed; new milestone)
```

- A discovery may take days; a milestone may take hours.
- A spawned FEAT enters `BACKLOG.md` (the unassigned pool),
  then a milestone-plan file picks it up.
- A DISC is milestone-independent — it never appears in
  `MILESTONE-<x>.<y>.<z>.md`.

## 4. The verb surface

```
project discover <topic>             # create DISC at phase: describe
project discover --advance <id>      # describe → ideate (or ideate → done)
project discover --link <id> FEAT-N  # link a spawned FEAT (reciprocal)
project discover --status            # list active + closed discoveries
```

Internally implemented at `libexec/project/discover` (FEAT-196).
The `--link` writes both directions: the DISC's
`spawned_features` list and the FEAT's `spawned_from` field.

## 5. Phase walkthrough

### 5a. `describe`

Output of `project discover <topic>` is a fresh DISC at
`issues/discovery/<NNN>-<slug>.md` with two empty section
headings (`## Describe` + `## Ideate`) and a placeholder
`## Spawned features` list.

Fill `## Describe` with:

- **Problem statement** — 1-3 paragraphs. Plain language.
- **Evidence** — links to existing code, bug tickets, prior
  conversations, user complaints. Specific, not anecdotal.
- **Audit results** if any (`operations/audit.md` might have
  surfaced the issue).
- **User impact** — who feels this? When?

Don't propose solutions in `describe`. That's the next phase.

### 5b. `ideate`

`project discover --advance <id>` transitions describe → ideate
**iff** the `## Describe` section is non-empty (gate-enforced).

Fill `## Ideate` with:

- **Candidate solutions** — divergent. List the obvious 3-5
  options including the cheapest and the most ambitious.
- **Trade-offs** per candidate — what does each cost / unlock?
- **Convergence** — pick which subset to actually ship.

Then **mint the FEAT tickets**. For each one:

1. Write the FEAT file at `issues/feature/<NNN>-<slug>.md`
   per `issues/feature/discovery.md` (frontmatter, AS-A, ACs, etc.).
2. Link it back to the DISC:
   ```
   project discover --link <DISC-id> FEAT-<NNN>
   ```
   This writes `spawned_features: [FEAT-NNN]` into the DISC
   frontmatter and `spawned_from: DISC-NNN` into the FEAT.

### 5c. `done`

`project discover --advance <id>` transitions ideate → done iff:

- `## Ideate` is non-empty, AND
- `spawned_features` has at least one entry

The DISC file is `git mv`'d to `issues/discovery/done/`. The
spawned FEATs are now in `BACKLOG.md` waiting for a milestone
plan to claim them.

## 6. Gate refusals — what to do when --advance refuses

| Refusal | Why | Fix |
|---|---|---|
| `## Describe is empty` | The section still contains only the template placeholder | Edit the file; fill the section with real content |
| `## Ideate is empty` | Same for the Ideate section | Same fix |
| `no spawned features recorded` | You haven't minted any FEATs yet | File the FEAT file(s); then `project discover --link <DISC-id> FEAT-N` |

## 7. Audit interaction

`operations/audit.md` § 3 is extended to verify the DISC → FEAT
linkage:

- Every FEAT carrying `spawned_from: DISC-NNN` resolves to an
  existing DISC under `issues/discovery/done/` (the DISC closed
  before the FEAT got built, otherwise the spawn was premature).
- Every DISC listing `spawned_features: [FEAT-N, ...]` resolves
  the named FEATs.

Orphans surface during the quarterly audit (or any time the
question "why does this exist?" comes up).

## 8. Worked example

```
# Stage 1 — describe
$ project discover "agents drift on long milestones"
DISC-001
$ $EDITOR issues/discovery/001-agents-drift-on-long-milestones.md
# Fill ## Describe with: who drifts, how, what they cost us, evidence.

# Stage 2 — ideate
$ project discover --advance DISC-001
discover - info: DISC-001: describe → ideate
$ $EDITOR issues/discovery/001-agents-drift-on-long-milestones.md
# Fill ## Ideate with candidate solutions (Stop hook? Periodic
# re-grounding? Manual checkpoint?), trade-offs, convergence.

# Stage 3 — mint FEATs
$ # write issues/feature/187-project-supervise.md per issues/feature/discovery.md
$ project discover --link DISC-001 FEAT-187
discover - info: DISC-001: linked FEAT-187 (spawned_features = [FEAT-187])
discover - info: FEAT-187: spawned_from = DISC-001

# Stage 4 — done
$ project discover --advance DISC-001
discover - info: DISC-001: ideate → done (relocated to issues/discovery/done/001-…)

# Now FEAT-187 is in BACKLOG.md; a future MILESTONE-<ver>.md picks it up.
```

## 9. Cross-references

- `issues/feature/discovery.md` — the delivery track that consumes spawned FEATs
- `operations/milestone.md` — how FEATs get assigned to milestones
- `operations/audit.md` § 3 — the `spawned_from` linkage check
- `issues/bug/design.md` § 1 — Design-issue escape hatch (bug → DISC, not bug → FEAT)
- `.repo/project/skills/methodology/vmodel.md` § "Dual-track: Discovery + Delivery"
- `.repo/project/skills/convention/tickets.md` → "Discovery ticket" — frontmatter schema

## 10. What this is *not*

- **Not classical design thinking.** No Empathize, Prototype, or Test phases.
- **Not a milestone-bound phase.** DISCs are continuous and milestone-independent.
- **Not retroactive.** Pre-FEAT-196 FEATs do not need a DISC parent.
  The `spawned_from` field is optional on FEATs.
- **Not a brainstorming substitute for the Design phase of a
  specific FEAT.** Once a FEAT is filed, its Design phase
  (`issues/feature/design.md`) pins implementation choices.