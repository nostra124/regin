---
name: bug-release
description: |
  Bug release phase — typically patch version, minimal
  ceremony. Trigger when a bug merges that changes
  user-visible behaviour.
---

# Bug release phase

## 1. What this phase produces

Release for bugs is simpler than features:

- **VERSION patch bump** — if user-visible behaviour changed
- **Git tag** — `v<x>.<y>.<z+1>` (or `v<x>.<y>.z+5` for half-step)
- **Changelog entry** — BUG line in CHANGELOG

Output: version tag (if bumped), changelog updated.

## 2. When VERSION is bumped for bugs

| Bug type | Bump VERSION? |
|----------|---------------|
| User-visible fix (CLI, output, behaviour) | Yes — patch |
| Internal fix (refactor, optimization) | No |
| Test-only fix | No |
| Doc fix | No |

Most bugs that reach production need a VERSION bump — they fix something the user sees.

## 3. Half-step convention

This collection uses the **half-step** convention:

```
X.Y.0 → X.Y.5 → X.Y.6 → ...
```

Not `X.Y.1` → `X.Y.2`. The `.5` signals "patch release" distinct from `.0` (feature).

## 4. Bug release flow

```
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│  Bug merged  │ → │ Bump VERSION │ → │    Tag       │
│  to master   │   │  (if needed) │   │  (if bumped) │
└──────────────┘   └──────────────┘   └──────────────┘
```

### 4.1 Decide bump

Check the bug's Resolution section:

```markdown
## Resolution

- ✅ Root cause: typo in flag parsing
- ✅ Fix: corrected flag parsing
- ✅ Regression test: tests/unit/project :: project-flag-parsing
```

If the fix changes CLI behaviour, output format, or error messages → bump.

### 4.2 Bump and tag (if needed)

```bash
# Edit VERSION
echo "X.Y.5" > VERSION

# Update test pin (path/extension is language-specific —
# see .repo/project/skills/language/<lang>.md)
$EDITOR tests/unit/project.<ext>
# Change the version assertion

# Commit
git commit -am "bump VERSION to X.Y.5 (patch)"

# Tag
make package VERSION=X.Y.5
```

## 5. Changelog

Add entry to `CHANGELOG.md`:

```markdown
## [X.Y.5] - 2024-01-15

### Fixed
- BUG-NNN: brief description of the fix
```

Changelog entries are **user-facing**. Internal bugs don't get changelog entries if VERSION wasn't bumped.

## 6. Milestone closure

Individual bugs don't close milestones. But if this bug was the **last ticket** in a milestone:

1. Run full test matrix on integration branch.
2. Merge integration → master.
3. Delete milestone plan file.
4. Delete integration branch.

See `operations/milestone.md` § 5 for full milestone closure.

## 7. Hotfixes

Critical bugs may ship as **hotfixes** — direct merges to master without milestone integration branch:

| Normal path | Hotfix path |
|-------------|-------------|
| Branch off integration | Branch off master |
| Merge to integration, then to master at milestone close | Merge directly to master |
| Milestone plan tracks it | No milestone plan (or retrospective FEAT filed) |

Hotfixes still:
- Follow red-then-green TDD
- Get CI verification
- May or may not bump VERSION (same rules apply)

## 8. Patch milestones

A milestone plan can be **all patches**:

```
issues/MILESTONE-X.Y.5.md
```

This groups multiple bug fixes into a single release. The process is the same as any milestone — integration branch, phase PRs, final merge.

## 9. Cross-references

- VERSION workflow: `.repo/project/skills/version.md`
- Milestone closure: `.repo/project/skills/operations/milestone.md` § 5
- Hotfix path: `.repo/project/skills/issues/bug/test.md` § 9
- Changelog format: `.repo/project/skills/convention/tickets.md` → "Changelog"