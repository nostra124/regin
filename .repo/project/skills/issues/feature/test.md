---
name: feature-test
description: |
  Test phase for feature tickets. CI green, SIT/PIT
  verification, PR auto-merge. Trigger after Build
  when the PR is open and CI is running.
---

# Feature test phase

## 1. What this phase produces

Test phase verifies the implementation:

- **CI green** — GitHub Actions unit tests pass
- **SIT green** — System Integration Tests pass (if scoped)
- **PIT green** — Production Integration Tests pass (if scoped)
- **PR merged** — auto-merge fires or manual merge
- **Ticket moved to done/** — `issues/feature/done/<NNN>-<slug>.md`

Output: merged commit on target branch, `phase: done`, `status: done`.

## 2. Entry conditions

Enter Test when:

- Build phase complete (PR open, `phase: build`)
- Subscribed to PR activity
- Waiting on CI

## 3. Test flow

```
   CI green
      │
      ▼
┌──────────┐     ┌──────────┐     ┌──────────┐
│  Unit    │ →   │   SIT    │ →   │   PIT    │
│  (CI)    │     │ (podman) │     │ (nightly)│
└──────────┘     └──────────┘     └──────────┘
      │                │                │
      └────────────────┴────────────────┘
                       │
                       ▼
               ┌────────────┐
               │ Auto-merge │
               │  or manual │
               └────────────┘
                       │
                       ▼
               ┌────────────┐
               │   Done     │
               └────────────┘
```

## 4. Test layers

| Layer | Path | Runner | When | Scope |
|-------|------|--------|------|-------|
| Unit | `tests/unit/` | CI on every PR | Always | All features |
| SIT | `tests/sit/` | podman, optional locally | Additive features | External deps |
| PIT | `tests/pit/` | nightly, post-merge | Multi-host | Production scenarios |

### 4.1 Unit tests (required)

CI runs `make check-unit` on every PR. A failure posts the log as a PR comment.

If CI red:
1. Read the failure comment
2. Switch to bug flow (`issues/bug/discovery.md`)
3. Write a red test, fix, prove green, push

### 4.2 SIT (if scoped)

Run locally with `make check-sit` if podman is available.

- **Additive features** require SIT.
- **Internal refactors** don't.
- **Soft-skip** if podman is missing — note in PR body.

After merge, SIT should be green on the integration branch.

### 4.3 PIT (if scoped)

PIT runs nightly or post-merge. It's not a PR gate.

- Features touching **real backends** need PIT.
- SIT exercises containers; PIT exercises production.

If PIT red post-merge, that's a **production incident** — file critical BUG.

## 5. CI wait pattern

After pushing:

1. **End the turn.** Don't poll.
2. Webhook event wakes the session (green or red).
3. On green: arm auto-merge (next section).
4. On red: switch to bug flow.

```markdown
# Don't do this:
while ! ci_green; do sleep 30; done

# Do this:
subscribe_pr_activity
# ... end turn ...
# Session wakes on webhook event
```

## 6. Auto-merge

When CI green:

| VERSION | Auto-merge? |
|---------|-------------|
| Pre-1.0 | Yes — armed by default |
| Post-1.0 | Ask first |

```
mcp__github__enable_pr_auto_merge(owner, repo, pullNumber, mergeMethod="SQUASH")
```

GitHub holds until CI passes, then merges automatically.
The session receives the `Outcome: merged` event.

If the repo toggle is off, fall back to:

```
mcp__github__merge_pull_request(owner, repo, pullNumber, merge_method="squash")
```

See `operations/automerging.md` for full gates.

## 7. On-merge cleanup

When merged:

1. **Move the ticket:**

```bash
git mv issues/feature/<NNN>-<slug>.md issues/feature/done/
```

2. **Flip frontmatter:**

```yaml
status: done
phase: done
```

3. **Append Resolution:**

```markdown
## Resolution

- ✅ <AC 1 passed>
- ✅ <AC 2 passed>
- ...
```

4. **Commit and push.**

## 8. Exit criteria

Test phase ends (and ticket done) when:

1. CI green
2. SIT green (if scoped)
3. PR merged
4. Ticket moved to `issues/feature/done/`
5. `status: done`, `phase: done` in frontmatter
6. Resolution section populated

No explicit transition command — on-merge cleanup sets `phase: done`.

## 9. Cross-references

- Previous phase: `issues/feature/build.md`
- Bug flow on CI red: `issues/bug/discovery.md`
- Auto-merge rules: `operations/automerging.md`
- Test layers: `policy/testing.md`
- SIT/PIT infrastructure: `.repo/project/skills/testing.md`