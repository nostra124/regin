---
name: project-reviewer
description: |
  Review a pull request against `project`'s conventions.
  Trigger when the user asks for a PR review, a code
  review, or a pre-merge sanity check. Reads diffs;
  produces inline review-style feedback; does not
  mutate.
---

# `project-reviewer` skill

## 1. Design principles

- **Cite, don't opine.** Every comment cites a CLAUDE.md
  section, an AGENTS.md topic, or a specific issue.
  Personal taste is not grounds for a review comment.
- **Severity ladder.** nit / suggestion / blocker.
  Blockers are rare and must point to a documented
  rule violation.
- **No mutation.** The reviewer never pushes, merges,
  or modifies the branch.

## 2. The convention checklist

| Topic               | Rule                                                  |
|---------------------|-------------------------------------------------------|
| Umbrella structure  | sub-services under `libexec/<umbrella>/`, not `bin/`  |
| Dispatcher          | every `command:<verb>` has a matching `help:<verb>`   |
| Test coverage       | new `command:*` ⇒ new `@test` in `tests/unit/`        |
| Semver              | public-API change ⇒ VERSION bump + version-pin test update |
| No-shared-lib       | runtime callouts only to declared deps (CLAUDE.md §4) |
| Issue refs          | every `FEAT-NNN` / `BUG-NNN` resolves to a file       |
| Bug-before-feature  | bugs ranked above features at the same priority       |

## 3. Workflow recipes

1. **Review a PR diff.**

       project review <PR-url>
       # → per-file comments with severity tags

2. **Pre-PR sanity pass.**

       project review HEAD~5..HEAD
       # → review your own branch before pushing

3. **Verdict-only mode.**

       project review --verdict <PR-url>
       # → ready / needs-work, with reasons

## 4. Guardrails

1. **Never push or merge.** That's the user's call.
2. **Don't fabricate rules.** If the rulebook doesn't
   cover something, flag the gap as an info-level note
   plus a doc issue. Don't invent a convention.
3. **Reserve `blocker`.** Use only for rule violations
   (missing test for new command, version pin not
   updated, dispatcher missing help). Everything else
   is `suggestion` or `nit`.

## 5. Where to read more

- Project root `CLAUDE.md`
- `AGENTS.md` (intro + topic index)
- `.repo/project/skills/language/<lang>.md`
