---
name: bug-build
description: |
  Bug build phase — implement the fix with TDD. Hard rule:
  the failing unit test is committed BEFORE the fix.
  Trigger when a BUG has been reproduced and phase: open.
---

# Bug build phase

## 1. What this phase produces

Build phase for bugs implements the fix:

- **Failing test** — unit test capturing the defect
- **Fix** — code change that makes the test pass
- **PR opened ready** — not draft, local gates green

Output: branch with red-then-green commits, PR open, `phase: build`.

## 2. The contract (red → green)

Every bug fix follows **red → green**:

1. **Reproduce** the defect.
2. **File** a `BUG-NNN` ticket.
3. **Write** a unit test that captures the defect. Run it; it must **fail** (proving reproduction).
4. **Commit the test** under a "BUG-NNN: reproduce" commit.
5. **Fix** the code.
6. **Run** the test; it must now **pass**.
7. **Commit the fix** under "BUG-NNN: fix" referencing the test commit.
8. **Push.**

Skipping step 3 is forbidden — see §6 for the narrow exception.

## 3. Build flow

```
┌─────────────┐   ┌────────────█   ┌─────────────┐   ┌─────────────┐
│  Red test   │ → │ Commit test│ → │    Fix      │ → │  Green test │
│ (prove fail)│   │            │   │             │   │ (prove pass)│
└─────────────┘   └────────────┘   └─────────────┘   └─────────────┘
```

### 3.1 Write the failing test

Add a regression test to the unit-test file for this package
under `tests/unit/<pkg>` (language-specific file extension and
syntax — see `.repo/project/skills/language/<lang>.md`). The
test should:

- Invoke the command that triggers the bug
- Assert the expected behaviour (status, output, side effect)
- Carry `BUG-NNN` in its name

**Always include BUG-NNN in the test name** — it's the audit trail.

Run it:

```bash
make check-unit
```

It must fail. If it passes, you haven't reproduced the bug.

### 3.2 Commit the test

```bash
git add tests/unit/<pkg>.<ext>
git commit -m "BUG-NNN: reproduce <short description>"
```

This commit is the proof the bug exists.

### 3.3 Fix the code

Implement the fix in `libexec/`, `bin/`, or relevant module.

### 3.4 Prove green

```bash
make check-unit
```

The test you added must now pass. All other tests must also pass.

### 3.5 Commit the fix

```bash
git add <changed files>
git commit -m "BUG-NNN: fix <short description>"
```

## 4. Branch naming

```
claude/bug-<NNN>-<slug>
```

Example:

```
claude/bug-167-pid-print-race
```

## 5. CI red on existing PR

When CI fails on a PR you're working on:

1. Read the test output from the PR comment.
2. Find the failing-test line and the assertion that fired.
3. **Don't** suppress the failure or widen tolerance.
4. If it's environment (missing tool, transient network), retry first.
5. Otherwise, file BUG-NNN, write red test, fix, prove green, push.

After **three** red-loop attempts on the same failure, pause and ask the user — the failure is probably architectural.

## 6. The narrow exception

If a bug genuinely **cannot** be expressed as a unit test (e.g.
a lint-only fix proven by the linter, a build-system path fix
proven only by SIT), the test addition can be a deliberate skip
(`skip "BUG-NNN: <reason>"` or the language's equivalent — see
`.repo/project/skills/language/<lang>.md`).

The skip must:
- Point at the specific reason
- Reference a follow-up that will remove it

This is rare. Default to a real test.

## 7. When CI goes red

From `issues/bug/discovery.md` § 5:

1. Read the failure log from the PR comment.
2. File BUG-NNN (or use existing if already filed).
3. Write red test → commit → fix → prove green → commit.
4. Push; CI re-runs.
5. On green, auto-merge fires.

## 8. Exit criteria

Build phase ends when:

1. Failing test committed (red)
2. Fix committed (green)
3. All tests pass locally
4. PR opened ready (not draft)
5. Subscribed to PR activity

Then transition to Test:

```
project transition <id> test
```

## 9. Guardrails

1. **Never** push a fix without the failing test committed first (or in the same PR, in the correct order).
2. **Never** skip a test to make CI green. Find the root cause.
3. **Never** suppress a `warn` or `error` log line to mask a flaky test — fix the flake.
4. **Never** widen a test's tolerance to make a fix "fit". Make the fix match the contract.
5. **Always** include the BUG-NNN id in the test name.

## 10. Cross-references

- Previous phase: `issues/bug/discovery.md` (or `issues/bug/design.md` if used)
- Next phase: `issues/bug/test.md`
- Feature build (for comparison): `issues/feature/build.md`
- CI failure handling: `issues/bug/test.md`