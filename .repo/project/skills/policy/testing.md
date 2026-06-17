# Test coverage + local-first execution

> Binding rules. Violations block PRs from opening or merging.

## Test coverage matrix per change kind

The agent **must not** open the PR until the matching tests are
included in the diff:

| Change kind | Required test changes | Notes |
|---|---|---|
| **Bug fix** | new / un-skipped unit test | the regression test that proves the fix; gated `skip "BUG-NNN: ..."` while the bug is open, un-skipped in the fix PR |
| **Feature (additive)** | new unit test **and** SIT suite | unit pins the surface; SIT proves the integration |
| **Feature with external integration** | unit + SIT + PIT | PIT only when the feature touches a real cloud / mainnet / external service |
| **Refactor** | (existing tests still pass) | no new tests required; coverage must not drop |
| **Docs / scaffolding** | (existing tests still pass) | no new tests required |

The classification belongs in the ticket's frontmatter (`type:
feature` or `type: bug`); the change kind for refactor / docs is
inferred from the diff.

**Hard-stop** — if a bug-fix PR ships no `tests/unit/*` change,
the agent must not open the PR. Same for a feature PR without
unit + SIT additions.

**Bug-fix ordering is TDD** — the failing test commit lands
*before* the fix commit. See `issues/bug/discovery.md` for the full
red → green protocol. The feature counterpart (ticket → branch
→ tests → gates → ready PR → auto-merge) lives in
`issues/feature/design.md`.

If a bug genuinely cannot be expressed as a unit test (e.g.
parser-only fix proven by lint), the test addition can be a
deliberate `skip "BUG-NNN: <reason>"` annotation pointing at the
specific gap. The exception is rare; the default is a real test.

## Local-first execution

CI is expensive. The agent runs the full gate stack **locally**
before opening the PR:

| Gate | Command | Skipped when |
|---|---|---|
| lint | `make lint` | the linter is not installed (note in PR body); concrete linter is language-dependent — see `.repo/project/skills/language/<lang>.md` |
| unit + vectors | `make check` | the unit test runner is not installed (note in PR body); concrete runner is language-dependent — see `.repo/project/skills/language/<lang>.md` |
| SIT | `make check-sit` | podman not installed (note in PR body) |
| PIT | `make check-pit` | podman missing OR credentials absent (note in PR body) |
| coverage | `make coverage` | the coverage tool is not installed (informational; PR not blocked) |

**Hard-stop** — a tier that's available locally but **failing**
blocks the PR open. A tier that's locally **unavailable**
(missing tool) soft-skips with an explicit body note:

> "SIT skipped locally — podman not installed; relies on CI."

That note is mandatory. It makes the gap auditable; reviewers (or
CI) can confirm the missing tier ran in the right environment.

**CI is the safety net, not the discovery mechanism.** A red CI
that the agent could have caught locally is an agent bug.

## CI failure surface (agent-readable)

Every CI tier (lint, unit + vectors + SIT, coverage, PIT) **posts
its failure summary to the PR as a comment** when red. The PR
comment is the canonical agent-readable failure log — it works
regardless of whether the GitHub MCP integration has
`actions:read` permission.

| Tier | Triggered by | Fail path |
|---|---|---|
| lint | every push / PR | `gh pr comment` with lint output |
| check (unit + vectors + SIT) | every push / PR | `gh pr comment` with failing-test lines + tail of the test log |
| coverage | every push / PR | `gh pr comment` notice (non-blocking) |
| PIT | nightly schedule + manual dispatch | `gh issue create` (no PR for cron-only events) |

When the agent is subscribed to PR activity
(`mcp__github__subscribe_pr_activity`), the CI failure comment
arrives as a `<github-webhook-activity>` event in the conversation
directly — no read call needed.

For nightly PIT failures (which land as issues, not PR comments),
use `mcp__github__list_issues`.

Do not rely on `get_status` / `get_check_runs` as a status oracle,
and **never poll them** waiting for CI to complete — webhook events
handle both green and red outcomes (see "Never poll for CI" in
`policy/merging.md`).

The concrete implementation pattern (workflow YAML, the
`gh pr comment` step, the pre-push hook that gates locally on the
same layers) lives in **`.repo/project/skills/testing.md`**. New
projects MUST ship the testing infrastructure described there — at
minimum `hooks/pre-push`, `.github/workflows/test.yml`, and the
`make install-hooks` target.