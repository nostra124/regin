---
name: feature-release
description: |
  Release phase for feature tickets. VERSION bump,
  tagging, milestone closure. Trigger when a feature
  that changes user-visible surface merges to master.
---

# Feature release phase

## 1. What this phase produces

Release phase ships the milestone:

- **VERSION bump** — if user-visible surface changed
- **Git tag** — `v<x>.<y>.<z>`
- **Milestone close** — delete plan file, merge integration branch
- **Backlog update** — remove shipped tickets from pool

Output: version tag on master, milestone plan deleted, shipped tickets in `done/`.

## 2. Entry conditions

Release happens at **milestone close**, not per-ticket:

- All tickets in the milestone are `phase: done`
- Exit criteria in the milestone plan are satisfied
- Integration branch is green (all tests pass)

## 3. Version bump rules

| Change kind | Bump | Example |
|-------------|------|---------|
| Bug fix | patch | `X.Y.0` → `X.Y.1` (or `X.Y.5`) |
| Additive feature | minor | `X.Y.0` → `X.(Y+1).0` |
| Breaking surface change | major | `X.0.0` → `(X+1).0.0` |

Internal-only changes (refactors, new tests, doc edits) don't bump VERSION.

## 4. VERSION bump checklist

In one PR (never split):

1. **Decide the level** (MAJOR / MINOR / PATCH).
2. **Edit `VERSION`** at repo root.
3. **Update the test pin** in the version-pin unit test
   (`tests/unit/<pkg>`, language-specific extension —
   see `.repo/project/skills/language/<lang>.md`). The
   pin asserts both that `project version` agrees with
   `VERSION` and that the value is the literal string
   `<new>`.

4. **Run tests:**

```bash
make check-unit
```

5. **Commit.** Title: `bump VERSION to <new> (<level>)`.
6. **After merge**, on the merge commit:

```bash
make package VERSION=<new>
```

This appends `<new>\t<sha>` to `.rpk/versions` and tags `v<new>`.

## 5. Milestone closure

Once all tickets in the milestone are done:

1. **Verify** all tickets in `MILESTONE-<x>.<y>.<z>.md` have matching files in `issues/<type>/done/`.
2. **Confirm exit criteria** — run full test matrix on integration branch.
3. **Flip** `status: open` → `status: done` in the milestone plan (optional).
4. **Merge** integration branch → master:

```bash
git checkout master
git merge claude/roadmap-<x>.<y>.<z>
```

5. **Delete** the milestone plan file:

```bash
git rm issues/MILESTONE-<x>.<y>.<z>.md
git commit -m "close milestone <x>.<y>.<z>"
```

6. **Delete** the integration branch:

```bash
git branch -d claude/roadmap-<x>.<y>.<z>
git push origin --delete claude/roadmap-<x>.<y>.<z>
```

7. **Tag** the release (if VERSION bumped):

```bash
make package VERSION=<new>
```

## 6. Integration branch lifecycle

| Milestone state | Integration branch |
|-----------------|-------------------|
| Open | Created off master (`claude/roadmap-<ver>`) |
| In-flight | Phase PRs target this branch |
| Closed | Merged to master, then deleted |

**Rules:**

- `master` sees only milestone-complete merges
- Phase PRs never target master directly
- Rolling back a milestone is one revert

## 7. Guardrails

1. **Never bump VERSION for internal-only changes.** Test-only or doc-only = no bump.
2. **Never tag manually.** Use `make package VERSION=...` — it updates `.rpk/versions`.
3. **Never skip the test pin update.** The pin IS the contract.
4. **Never merge integration branch before all tickets done.** Exit criteria must pass.

## 8. Cross-references

- VERSION workflow: `.repo/project/skills/version.md`
- Milestone management: `.repo/project/skills/operations/milestone.md`
- Integration branch rules: `.repo/project/skills/operations/milestone.md` § 2.2
- Semver routing: `.repo/project/skills/methodology/vmodel.md` § "Bug ↔ feature ↔ semver routing"