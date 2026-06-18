---
name: audit
description: |
  Traceability audit — verify every piece of code,
  every test, and every documented behaviour maps to
  at least one ticket (feature or bug). Trigger
  quarterly, before a major release, when onboarding
  a new sub-service, or whenever a "why does this
  exist?" question surfaces during review.
---

# `audit` skill

## 1. Goal

Each piece of shipped functionality should be
traceable to a ticket that documents *why* it exists.
The audit verifies the two-way mapping:

```
feature / bug ticket  ⇄  code + tests + docs
```

When the mapping breaks, one of three things has
happened:

1. **Orphan functionality** — code exists without a
   ticket. Either backfill a ticket (capturing the
   rationale that's currently implicit), or remove
   the code as dead weight.
2. **Phantom ticket** — a `done/` ticket has no
   corresponding code/test. Either the merge was
   incomplete, the feature was later deleted without
   updating the ticket, or the ticket was closed in
   error. Investigate and reconcile.
3. **Stale doc** — the ticket and the code agree, but
   the user-facing doc (man page, README, help text)
   no longer reflects reality. Open a follow-up
   ticket to refresh.

## 2. Scope

For each package, audit the following surfaces:

| Surface              | Where                            |
|----------------------|----------------------------------|
| Public commands      | `bin/<pkg>`, `command:*` in libexec |
| Public flags         | `getopts` loops + their handlers |
| Sub-services         | `libexec/<pkg>/<name>`           |
| Test cases           | files under `tests/unit/` (language-specific extension — see `.repo/project/skills/language/<lang>.md`) |
| Skills               | `.repo/project/skills/*.md`, `.repo/project/skills/<name>/SKILL.md` |
| Workflow files       | `.github/workflows/*.yml`        |
| Hook files           | `hooks/*`                        |
| Build verbs          | `Makefile.in` targets            |
| Documented behaviour | `share/man/man1/<pkg>.1`, `Readme.md` |

For each item in those surfaces, find at least one
ticket under `issues/{feature,bug}/done/` (or open
`issues/{feature,bug}/`) whose Acceptance Criteria
or Resolution covers it.

## 3. The audit loop

Run this read-only loop — it never mutates the tree:

```
1. enumerate surfaces        →  list every command, flag, test,
                                  skill, workflow, hook
2. for each item:
     find a ticket             →  grep -r 'FEAT-NNN\|BUG-NNN'
                                     issues/{feature,bug}/{,done/}/
                                + issues/MILESTONE-*.md
                                + look in PR commit messages
                                  (git log -S '<item>' --oneline)
3. classify the result        →  hit / orphan / phantom / stale
4. verify DISC ↔ FEAT linkage →  for every FEAT carrying
                                  spawned_from: DISC-NNN, the
                                  named DISC must exist under
                                  issues/discovery/done/.
                                  For every DISC listing
                                  spawned_features: [FEAT-N, …],
                                  each named FEAT must exist
                                  (either open or done).
5. produce a report           →  markdown table, one row per item
6. file follow-ups            →  one BUG-NNN per orphan to backfill
                                + one FEAT-NNN per phantom to
                                  decide-and-act
                                + one BUG-NNN per broken DISC↔FEAT
                                  link (dangling spawned_from or
                                  spawned_features reference)
```

The audit's output is **always advisory**. It
produces tickets; it does not modify the tree
itself. Apply fixes through the normal feature/bug
flow (`.repo/project/skills/issues/feature/design.md` /
`.repo/project/skills/issues/bug/discovery.md`).

### Discovery-track invariants (FEAT-196)

The dual-track model (Delivery + Discovery —
`.repo/project/skills/methodology/vmodel.md` →
"Dual-track: Discovery + Delivery") adds two
audit-checkable rules:

1. **Linkage is reciprocal.** If a FEAT lists
   `spawned_from: DISC-N`, the DISC must list that
   FEAT in `spawned_features`. Asymmetric linkage
   means one side was edited by hand without the
   other being updated.
2. **DISC closure precedes FEAT closure.** A
   `done/` FEAT's parent DISC should also be in
   `done/`. (A `done/` FEAT pointing at an
   *active* DISC means we shipped before
   discovery finished — possibly a legitimate
   case, but worth flagging.)

`spawned_from` is **optional** on FEATs — pre-
FEAT-196 features and small straightforward FEATs
don't need a discovery parent. The audit only
verifies links that are present, not that every
FEAT must have one.

## 4. Audit cadence

| Trigger                         | Audit depth        |
|----------------------------------|--------------------|
| Quarterly                        | full sweep         |
| Before a major (X.0.0) release   | full sweep         |
| Before a minor (X.Y.0) release   | new-since-last-minor |
| On suspicion ("why is this here?") | targeted item    |
| New sub-service merged           | the new sub-service |

The full sweep should take a single session: small
codebase, the surfaces table is finite, and the audit
loop is mechanical.

## 5. Report shape

```markdown
# Traceability audit — <date>

## Summary

- Surfaces inspected: N
- Hits: N
- Orphans: N
- Phantoms: N
- Stale docs: N

## Hits (every surface mapped to a ticket)

| Surface                 | Ticket(s)        |
|-------------------------|------------------|
| `project version`       | FEAT-005         |
| `project autogen`       | (initial import) |
| ...                     | ...              |

## Orphans (code without a ticket)

| Surface          | Notes                                     | Follow-up |
|------------------|-------------------------------------------|-----------|
| `task -q` flag   | undocumented; predates ticket conventions | BUG-NNN to backfill ticket or remove |

## Phantoms (tickets without code)

| Ticket          | Notes                            | Follow-up |
|-----------------|----------------------------------|-----------|
| FEAT-NNN        | Closed in PR #M but code reverted in PR #N | reopen + re-implement, or close as superseded |

## Stale docs

| Surface          | Doc | Discrepancy                | Follow-up |
|------------------|-----|----------------------------|-----------|
| `project rules`  | man | man page predates FEAT-164 | FEAT-NNN doc refresh |
```

Land the report in `docs/audits/audit-<YYYY-MM-DD>.md`
(or attach to a PR). The follow-up tickets go through
the normal `issues/` flow.

## 6. Special cases

### Initial import

Code that landed in the package's first commit
(before tickets were a thing) is exempt — but flag
the most prominent missing tickets and backfill them
as `FEAT-NNN [retroactive]` so future audits don't
re-flag the same items.

### Vendored references

Files under `share/doc/<pkg>/standards/` (FEAT-163,
pending) are vendored from upstream sources. They
don't need per-file tickets; one umbrella ticket
covers the vendoring policy.

### Tests

Each `@test` should be discoverable via either:

- a `(FEAT-NNN ...)` / `(BUG-NNN regression)` suffix
  in the test name, or
- a comment block at the top of the suite naming the
  ticket(s) it covers.

Tests without either are orphan tests — file
follow-up.

## 7. Guardrails

1. **Read-only.** The audit produces a report; it
   never edits code, tickets, or milestone plans directly.
2. **Don't backfill silently.** Every orphan gets a
   *visible* follow-up ticket. Adding the ticket
   without flagging the orphan defeats the audit's
   purpose.
3. **Don't reopen `done/` tickets.** If a phantom
   needs work, open a *new* ticket that supersedes
   the closed one; cite the original in the new
   ticket's Description.
4. **Tickets are not retroactively rewritten.** A
   shipped ticket's Acceptance Criteria reflect what
   was *promised*. If reality drifted, that's a new
   ticket, not an edit.
5. **One audit report per session.** The report is
   the artefact; don't fold multiple audits into one
   document.

## 8. Distinction from `project-auditor` skill

`.repo/project/skills/project-auditor.md` audits *health*
(test coverage, manifest freshness, dependency
hygiene). This skill audits *traceability* (every
surface ↔ at least one ticket). Run both
independently; they answer different questions.

## 9. Cross-references

- Where milestones map to tickets:
  `operations/milestone.md`
- Where ticket files live:
  `.repo/project/skills/convention/tickets.md` →
  "Ticket file"
- Test naming convention (`BUG-NNN regression`):
  `issues/bug/discovery.md` §7
- Health audit counterpart:
  `.repo/project/skills/project-auditor.md`
