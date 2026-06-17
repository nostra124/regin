---
name: project-auditor
description: |
  Audit a project's build / test / dependency health.
  Trigger when the user wants a punch-list of issues:
  missing manifests, stale lockfiles, broken backend
  markers, untested verbs, undocumented commands, dead
  issue references. Read-only — never mutates the tree.
---

# `project-auditor` skill

## 1. Design principles

- **Read-only.** The auditor inspects; it never edits,
  installs, packages, or otherwise mutates the tree.
- **Severity-graded.** Findings are info / warn / error,
  not pass / fail. The auditor produces a punch-list,
  not a verdict.
- **Citable.** Every finding cites either a CLAUDE.md
  section, an open issue, or a concrete file path.

## 2. The audit dimensions

For every backend `project` knows about:

| Dimension          | What to check                                   |
|--------------------|-------------------------------------------------|
| Manifest health    | parseable, no missing required fields           |
| Lockfile freshness | newer than manifest? matching package set?      |
| Test coverage      | every `command:*` has at least one `@test`      |
| Doc coverage       | every command has `help:<verb>`                 |
| Issue refs         | every `FEAT-NNN` / `BUG-NNN` resolves to a file |
| Backend tool       | autoreconf / cmake / cargo / etc. on PATH       |

## 3. Workflow recipes

1. **Audit a single project.**

       cd ~/src/myapp
       project audit                  # → markdown report

2. **Audit an umbrella tree.**

       project audit ~/src            # → aggregate report

3. **Audit dependency freshness.**

       project audit --deps           # → outdated table

## 4. Guardrails

1. **Read-only.** Never invoke `project install`, `project
   package`, `git push`, or any side-effectful verb. If a
   finding requires action, hand off to `project-author` or
   `project-packager`.
2. **Severity-graded findings.** Reserve `error` for
   genuine breakage (missing required manifest field,
   broken marker). Use `warn` for hygiene issues. Use
   `info` for nice-to-haves.
3. **No mass-fixes.** The auditor's output is a list; the
   user (or another skill) decides what to fix.

## 5. Where to read more

- `man project-audit` (pending — see FEAT-179)
- `share/doc/project/standards/` — vendored references
  (FEAT-163, pending)
- This package's `CLAUDE.md`
