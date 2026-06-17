---
name: version
description: |
  Bump and ship versions for `project`. Single
  canonical source: VERSION at the repo root. Trigger
  when about to cut a release, when behaviour changes
  in a way that should be visible to users, or when
  asked "what version are we on". Defines semver
  rules, the bump checklist, and the test contract.
---

# `version` skill

## 1. The single source of truth

The umbrella version lives in **`VERSION`** at the
repo root. One line, plain semver, e.g.:

    1.1.0

Everything else reads from there:

- `bin/project` reads `$repo_root/VERSION` (dev tree)
  or `$prefix/share/project/version` (installed).
- `./configure` validates `VERSION` exists and is
  semver before generating the Makefile.
- `make install` copies `VERSION` →
  `$prefix/share/project/version`.
- The unit test that pins the version (path
  conventionalised per language; e.g.
  `tests/unit/project` with the language's test-file
  extension — see
  `.repo/project/skills/language/<lang>.md`) pins
  both *self-consistency* (`project version` output
  matches `VERSION` contents) and *current value*
  (the literal string the team agreed on).

Sub-services in `libexec/project/*` ship their own
internal `VERSION='x.y.z'` strings — they release on
independent cycles. Don't conflate them with the
umbrella.

## 2. The semver rules (CLAUDE.md §8)

| Bump | Trigger                                            |
|------|----------------------------------------------------|
| MAJOR | Breaking change to a documented verb, flag, or file path. |
| MINOR | New verb / flag / sub-service added; no removal.   |
| PATCH | Bug fix; no surface change; tests pass on the old contract. |

Internal-only changes (refactors, new tests, doc
edits) don't move the version. The version-pin unit
test is the gate: if no test changes, no version bump.

## 3. The bump checklist

In one PR (never split — the test pin is the gate):

1. Decide the level (MAJOR / MINOR / PATCH).
2. Edit `VERSION` to the new value.
3. Update the pin in the version-pin unit test
   (`tests/unit/project` plus the
   language-specific extension; see
   `.repo/project/skills/language/<lang>.md`). The pin
   asserts both that `project version` agrees with
   `VERSION` and that the value is the literal
   string `<new>`.
4. `make check-unit` green.
5. Commit. Title: `bump VERSION to <new> (<level>)`.
6. After merge, on the merge commit, run:

       make package VERSION=<new>

   which appends `<new>\t<sha>` to `.rpk/versions`
   and tags `v<new>`.

## 4. Worked examples

1. **Fixing a bug in `bin/project`.**
   `1.1.0 → 1.1.1`. Same-PR test pin update.

2. **Adding `project backend` verb (FEAT-158).**
   `1.1.0 → 1.2.0`. New verb, no breakage.

3. **Removing the deprecated `project deploy` verb.**
   `1.x.x → 2.0.0`. Breaking change.

4. **Adding a new unit test file.**
   No bump. Internal-only.

5. **Fixing a bug in `libexec/project/task`.**
   `task`'s own VERSION='1.0.0' → '1.0.1' inside that
   file. Umbrella VERSION unchanged (task's surface is
   accessed via `project task` but task ships
   independently).

## 5. Guardrails

1. **Never edit `VERSION` without updating the test pin
   in the same commit.** The pin IS the contract per
   CLAUDE.md §8. A drift makes the test fail
   immediately, which is the desired behaviour.
2. **Never bump without a user-visible behaviour
   change.** If `git diff` shows only test or doc
   changes, there is no version bump.
3. **Don't tag releases manually.** Use
   `make package VERSION=...`. The history log in
   `.rpk/versions` is auditable, manual tags
   aren't.
4. **Don't write to `.rpk/version`.** That file has
   been removed; the canonical source is root
   `VERSION`. The `.rpk/versions` (plural) history
   log stays.

## 6. Where to read more

- `VERSION`                                  — canonical source
- `bin/project` lines 12–22                  — read path
- `tests/unit/project` (language-specific extension) — test contract; see `.repo/project/skills/language/<lang>.md`
- `Makefile.in` → `install` / `package`      — install & release flows
- `.rpk/versions`                            — historical log
- CLAUDE.md §8 (semver) and §10 (this skill)
