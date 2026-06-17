---
name: feature-build
description: |
  Build phase for feature tickets. Code implementation,
  unit tests, PR opened ready. Trigger when starting
  implementation on a FEAT with phase: design.
---

# Feature build phase

## 1. What this phase produces

Build phase implements the feature:

- **Code** — the implementation in `libexec/`, `bin/`, or relevant dirs
- **Unit tests** — files under `tests/unit/` covering new surface
  (language-specific file extension; see
  `.repo/project/skills/language/<lang>.md`)
- **PR opened ready** — not draft, local gates green

Output: branch `claude/<feat-id>-<slug>` with commits, PR open, `phase: build`.

## 2. Entry conditions

Enter Build when:

- Design phase is complete (`## Design` filled, sizing set)
- The FEAT is assigned to an open milestone
- No higher-priority bugs in that milestone
- The integration branch exists (if milestone > first milestone)

## 3. Branch naming

```
claude/<feat-id>-<slug>
```

Examples:

```
claude/feat-158-multi-backend-dispatch
claude/feat-173-build-tests
```

The `claude/` prefix marks an agent-owned branch (auto-merge eligible).

## 4. Build flow

```
┌──────────┐   ┌────────┐   ┌─────────┐   ┌────────┐
│  Ticket  │ → │ Branch │ → │  Code + │ → │  Local │
│  (file)  │   │        │   │  Tests  │   │  Gates │
└──────────┘   └────────┘   └─────────┘   └────────┘
```

### 4.1 Read the ticket

Open `issues/feature/<NNN>-<slug>.md`. Verify:

- `## Design` section exists and is concrete
- AC is testable
- Complexity/estimates are set

If any are missing, **go back to Design** — don't build unspecified work.

### 4.2 Branch

```bash
git checkout -b claude/feat-<NNN>-<slug> <integration-branch-or-master>
```

- For milestone work: branch off `<agent>/roadmap-<x>.<y>.<z>`
- For rules/doc-only: branch off `master`

### 4.3 Code + tests

**Order depends on change type:**

| Change type | Test order |
|-------------|------------|
| New verb/flag | Tests first (pin the surface) |
| Add flag to existing verb | Tests after (prove coverage) |
| Internal refactor | Tests must pass, no new tests required |

Coverage per change kind:

| Change kind | Required tests |
|-------------|----------------|
| Additive feature | unit + SIT |
| Feature with external integration | unit + SIT + PIT |
| Internal refactor | (existing tests pass) |

### 4.4 Local gates

Run **before** opening the PR:

```bash
make lint           # the linter (soft-skip if absent); concrete linter is
                    # language-dependent — see .repo/project/skills/language/<lang>.md
make check          # make check-unit (the unit test suite)
make check-sit      # podman SIT (soft-skip without podman)
make check-pit      # podman PIT (soft-skip without credentials)
```

The pre-push hook runs these automatically. Any failure blocks push.

## 5. PR

Open **ready** (not draft) when local gates pass.

**PR title:**

```
FEAT-NNN: <one-line description>
```

**PR body:**

```markdown
## Summary
<2-5 bullets: what changed, why>

## Verification
- make lint → ok
- make check → N tests
- make check-sit → ok / soft-skipped (no podman)

## Test plan
- [ ] CI: make check green
- [ ] (manual) <feature-specific>
```

**Commit messages** include the session trailer:

```
FEAT-NNN: <one-liner>

<body>

Session: claude-code:<session-id>
https://claude.ai/code/session_<id>
```

## 6. Subscribe and wait

After opening:

```
mcp__github__subscribe_pr_activity
```

Then **end the turn**. Don't poll, don't sleep, don't loop on CI status.
Webhook events wake the session.

## 7. Exit criteria

Build phase ends when:

1. Branch created with correct naming
2. Code + tests committed
3. Local gates green
4. PR opened ready (not draft)
5. Subscribed to PR activity

Then transition to Test:

```
project transition <id> test
```

## 8. If CI goes red

See `issues/bug/discovery.md` — the bug flow handles CI failures:
file BUG, write red test, fix, prove green, push.

After **three** red-loop attempts, pause and ask the user.

## 9. Cross-references

- Previous phase: `issues/feature/design.md`
- Next phase: `issues/feature/test.md`
- Bug flow: `issues/bug/discovery.md`
- Auto-merge: `operations/automerging.md`
- Test layers: `policy/testing.md`