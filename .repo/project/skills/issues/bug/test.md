---
name: bug-test
description: |
  Bug test phase — CI green, verification, PR merge.
  Trigger after Build when the PR is open and CI is running.
---

# Bug test phase

## 1. What this phase produces

Test phase for bugs verifies the fix:

- **CI green** — GitHub Actions unit tests pass (including the regression test)
- **No regressions** — all existing tests still pass
- **PR merged** — auto-merge fires or manual merge
- **Ticket moved to done/** — `issues/bug/done/<NNN>-<slug>.md`

Output: merged commit on target branch, `phase: done`, `status: done`.

## 2. Entry conditions

Enter Test when:

- Build phase complete (PR open, `phase: build`)
- Red test committed in Build phase
- Fix committed
- Subscribed to PR activity
- Waiting on CI

## 3. Test flow

```
   CI runs
      │
      ▼
┌──────────┐     ┌──────────────┐     ┌───────────────┐
│  Unit    │ →   │  Regression  │ →   │   All other   │
│  tests   │     │  test passes │     │   tests pass  │
└──────────┘     └──────────────┘     └───────────────┘
      │                   │                   │
      └───────────────────┴───────────────────┘
                          │
                          ▼
                  ┌────────────┐
                  │ Auto-merge │
                  └────────────┘
                          │
                          ▼
                  ┌────────────┐
                  │    Done    │
                  └────────────┘
```

## 4. Verification checklist

The bug fix must verify:

| Check | How |
|-------|-----|
| Regression test passes | `make check-unit` (filter to the BUG-NNN test per `.repo/project/skills/language/<lang>.md`) |
| All unit tests pass | `make check-unit` |
| No new warnings | `make lint` clean |
| SIT passes (if scoped) | `make check-sit` |

## 5. CI red handling

If CI fails on the bug fix PR:

1. **Don't disable auto-merge.** Keep it armed; GitHub retried on new commits.
2. Read the failure log from the PR comment.
3. If the regression test itself fails:
   - The fix didn't work → go back to Build, adjust the fix.
4. If a different test fails:
   - The fix broke something → go back to Build, adjust the fix.
5. Push fix commit; CI re-runs.
6. On green, auto-merge fires.

**Three-attempt limit**: after three red-loop attempts, disable auto-merge and ask the user. The fix is probably touching something larger.

## 6. SIT/PIT for bugs

| Change kind | SIT required? | PIT required? |
|-------------|---------------|---------------|
| Local fix, no external deps | No | No |
| Fix touches external interface | Yes | If production scenario |
| Fix in build/deploy scripts | Yes | No (build scripts are SIT coverage) |

Most bugs are local fixes. If the bug was in an external interface, the SIT that covers that interface must pass.

## 7. On-merge cleanup

When merged:

1. **Move the ticket:**

```bash
git mv issues/bug/<NNN>-<slug>.md issues/bug/done/
```

2. **Flip frontmatter:**

```yaml
status: done
phase: done
```

3. **Append Resolution:**

```markdown
## Resolution

- ✅ Root cause: <what was wrong>
- ✅ Fix: <what changed>
- ✅ Regression test: tests/unit/<pkg> :: <test-name>
```

4. **Commit and push.**

## 8. Exit criteria

Test phase ends (and ticket done) when:

1. CI green (regression test + all tests)
2. No new lint warnings
3. PR merged
4. Ticket moved to `issues/bug/done/`
5. `status: done`, `phase: done` in frontmatter
6. Resolution section populated

No explicit transition command — on-merge cleanup sets `phase: done`.

## 9. Hotfix path (critical bugs)

For critical production bugs that need immediate fix on master:

1. Skip milestone integration branch — branch directly off master.
2. Same red-then-green TDD flow.
3. PR targets master, not integration branch.
4. Fast-track merge after CI green.
5. File a retrospective FEAT if the hotfix warrants process review.

The hotfix is **still a BUG** — it goes through the same test phase. The only difference is branch targeting.

## 10. Cross-references

- Previous phase: `issues/bug/build.md`
- Feature test (for comparison): `issues/feature/test.md`
- Auto-merge rules: `operations/automerging.md`
- Critical bugs: `operations/milestone.md` → hotfix exceptions