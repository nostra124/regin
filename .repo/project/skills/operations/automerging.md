---
name: automerging
description: |
  Hands-off PR merging once CI is green. Defines the
  gates that must hold, the MCP tool sequence, the
  one-time repo-level prerequisite, and the
  interaction with the bug-handling flow when CI goes
  red. Trigger when a PR has been opened ready, when
  CI events arrive for a subscribed PR, or when
  scoping whether a PR is auto-merge-eligible.
---

# `automerging` skill

## 1. Prerequisite (one-time, per repo)

Auto-merge must be enabled at the repository level
before the MCP API will accept the request. This is
a UI toggle the user owns:

> **GitHub → Settings → General → Pull Requests →
> ✅ Allow auto-merge**

`mcp__github__enable_pr_auto_merge` returns
"Auto-merge is not enabled for this repository" until
that toggle is on. Without it, the agent falls back
to a direct `mcp__github__merge_pull_request` call
on CI green (see §5).

## 2. The gates

Auto-merge fires when **all** of:

1. **CI green** on the PR's head commit (all required
   checks completed with `conclusion: success`).
2. **Not draft** — PR opened in ready state.
3. **No `state: changes_requested`** review pending.
4. **PR doesn't touch the shared-infrastructure list**
   (per `.repo/project/skills/policy/merging.md` →
   "Shared-infrastructure list"):
   - the linter's config (e.g. `.shellcheckrc`,
     `.clang-tidy`, `pyproject.toml`, `.eslintrc`,
     `Cargo.toml [lints]`, etc. — language-dependent;
     see `.repo/project/skills/language/<lang>.md`)
   - top-level `Makefile` lint / check gate logic
   - `.github/workflows/*.yml` (the CI definitions
     themselves)
   - `<pkg>/.rpk/depends/*`
   - `docs/templates/{Makefile.in,configure.in,
     install.in,CLAUDE.md.foundation}`
   - the rules tree: `<pkg>/AGENTS.md` and
     `<pkg>/.repo/project/skills/*`
5. **VERSION is pre-1.0.0** *or* the user has
   **explicitly authorised** auto-merge for this PR
   in the request.

If any gate fails, pause and ask the user. Don't
auto-merge "almost".

## 3. The MCP tool sequence

After opening the PR ready and subscribing
(`issues/feature/design.md` §5):

```
mcp__github__enable_pr_auto_merge(
    owner, repo, pullNumber,
    mergeMethod="SQUASH",   # repo default if omitted
)
```

That's it. GitHub holds the PR until CI passes, then
merges automatically. The session receives a final
`<github-webhook-activity>` event with `Outcome:
merged` once the merge fires.

Merge method: prefer **SQUASH** for agent PRs (one
ticket → one squashed commit on master keeps the
history flat). `MERGE` is fine for human-curated
multi-commit PRs.

## 4. CI red → bug flow

When the workflow's PR-comment failure log arrives
(`.repo/project/skills/testing.md` §3) while auto-merge is armed:

1. **Auto-merge stays armed.** GitHub re-evaluates on
   the next CI run. Don't disable it.
2. Switch to the **bugs** flow (`issues/bug/discovery.md`):
   file `BUG-NNN`, write the failing test, commit,
   fix, prove green, commit fix, push.
3. CI re-runs on the new head commit. On green, the
   armed auto-merge fires automatically.

Cap at **three** red-loop attempts on the same
failure (`.repo/project/skills/policy/merging.md` →
"Red CI loop policy"). After three, **disable**
auto-merge via `mcp__github__disable_pr_auto_merge`
and pause to ask the user — the failure is probably
architectural.

## 5. Fallback path (when the repo-level toggle is OFF)

This is the **default path for any repo where the Settings
toggle from §1 has not been flipped**. It's the procedure
that empirically works across many projects with no setup
beyond what `make install-hooks` provides.

### The flow

```
1. push commit       →  PR head SHA updated; CI run starts
2. open / update PR  →  mark ready (not draft); subscribe
3. end the turn      →  output a brief status line:
                          "PR ready; merge will fire on the
                           next prompt to this session after
                           CI completes."
4. session sleeps    →  no polling, no sleep
5. session wakes     →  triggered by EITHER:
                          (a) a webhook event (PR comment,
                              review, merge), OR
                          (b) the user's next prompt for any
                              reason
6. same-turn check   →  one mcp__github__pull_request_read
                        get_check_runs call
7. classify          →  queued/in_progress → status note, end
                                              turn again
                        completed/success   → merge_pull_request
                                              with squash
                        completed/failure   → bugs flow
                                              (issues/bug/discovery.md)
```

Empirical reality: CI-success webhook events are **not
reliably delivered** to a subscribed session on many host
configurations. Only the post-merge event consistently
arrives. The pattern above sidesteps that by treating the
user's next prompt as a wake-up signal of equal weight.

### Why this works "for a lot of projects"

- No repo-level configuration required.
- No agent-side sleep loop (still honours §8 "never poll").
- The single same-turn `get_check_runs` call in step 6 is
  *not* polling — it's verification immediately before a
  fallback merge, the same exception called out in §8.
- The merge happens at most one turn after CI green, which
  is human-acceptable latency for the kinds of changes this
  collection ships.

### When this gets old

Flip the Settings → General → Pull Requests → Allow
auto-merge toggle (§1). Then the **standard path** (§3)
applies — GitHub fires the merge on its own without needing
a session prompt to do it.

### Direct-merge invocation

```
mcp__github__merge_pull_request(
    owner, repo, pullNumber,
    merge_method="squash",
    commit_title="<TICKET-ID>: <one-line summary> (#<N>)",
    commit_message=<full body — closes the milestone if applicable>,
)
```

The `commit_title` MUST include the PR number suffix
`(#<N>)` to match the standard GitHub squash format.

## 6. Authorisation rules

| Scenario                                          | Auto-merge?            |
|---------------------------------------------------|------------------------|
| Pre-1.0 package, no shared-infra change           | Yes — armed by default |
| Pre-1.0 package, shared-infra change              | Ask first              |
| Post-1.0 package, no shared-infra change          | Ask first              |
| Post-1.0 package, shared-infra change             | Ask first              |
| User explicitly says "auto-merge this"            | Yes — single-use       |
| User explicitly says "merge once CI green"        | Yes — single-use       |

The user's authorisation is **single-use**: a
previous OK does not authorise later PRs. Each
occurrence re-asks (per
`.repo/project/skills/policy/conduct.md` → "Hard
'never autonomously' list").

## 7. On-merge cleanup

When the merge fires (signalled by `<github-webhook-activity>`
with `Outcome: merged`):

1. Move the ticket file → `issues/feature/done/` (or
   `issues/bug/done/` for bugs).
2. Flip frontmatter `status: open` → `status: done`.
3. Append a `## Resolution` section with per-AC
   outcomes (✅ / ❌ / ⚠️) — see
   `.repo/project/skills/convention/tickets.md` →
   "Ticket file".
4. If the merge bumped VERSION, run
   `make package VERSION=<new>` on master to tag
   the release (`.repo/project/skills/version.md` §3).

The session is automatically unsubscribed from PR
activity on merge — no manual `unsubscribe_pr_activity`
needed.

## 8. Never poll — wait for the webhook

After arming auto-merge (or after pushing a fix into
an armed PR), **end the turn**. Do not:

- `sleep` in Bash waiting for CI to finish
- Loop on `pull_request_read get_check_runs`
- Loop on `pull_request_read get_status`
- Run any "watcher" command that blocks the session

CI runtime is on the order of seconds-to-minutes and
the session does not need to be alive while GitHub
runs the checks. Holding the session open with a
`sleep` is a **session hang** — it wastes context,
blocks the agent from responding to the user, and
adds nothing.

The correct pattern is:

1. Subscribe via `mcp__github__subscribe_pr_activity`
   (once per PR).
2. Push commits as needed.
3. **End the turn.** Output a brief status line; stop
   tool use.
4. GitHub's webhook (CI completion, review comment,
   merge) delivers a `<github-webhook-activity>`
   event that **wakes** the session. The next turn
   starts with that event in hand.

This applies equally to **green** and **red** CI
outcomes. Don't poll to "catch the failure faster".
The red event arrives via the same webhook path and
wakes the session the moment CI completes. Polling
beats no value over the wait — it just costs
context.

The single exception: when you're about to call
`merge_pull_request` directly (the fallback path,
§5), verify CI is green in the *same* turn via one
`get_check_runs` call. That's a same-moment check,
not a wait.

### Wake-up signals

A session wakes from "end turn" via one of:

| Source                | Reliably fires?  |
|-----------------------|------------------|
| PR-merge webhook      | Yes              |
| PR-comment webhook    | Yes              |
| Review webhook        | Yes              |
| CI-completion webhook | **Not reliably** on many host configurations |
| User prompt           | Yes              |

Because the CI-completion webhook is not guaranteed,
the fallback path in §5 treats the *user's next
prompt* as a valid wake-up signal of equal weight to
a webhook. This is the documented procedure for any
repo whose Settings toggle from §1 is off.

### Poll-on-resume — the missed-webhook backstop

Webhook delivery is best-effort. A subscribed session
can wait forever for a green-CI event that GitHub
sent but the receiver dropped. The rule that prevents
this:

**Every time a session wakes during an open PR
watch — for any reason: a webhook event, a user
prompt, a stop-hook re-grounding — the first
tool call is one `mcp__github__pull_request_read`
with `method: get_check_runs` on the watched PR's
head SHA.** Branch on the result before doing
anything else:

| status     | action                                    |
|------------|-------------------------------------------|
| success    | merge now (the green webhook was missed)  |
| failure    | switch to bug flow (`issues/bug/discovery.md`) |
| in_progress / queued | end the turn; the *next* wake re-checks |

This is **not polling.** Polling is repeated
synchronous checks inside one turn. This is a single
verification at wake-time, identical in shape to
§5's same-turn pre-merge check. It costs one tool
call per wake, which is cheap, and it makes the
session physically incapable of getting stuck.

The empirical case this fixes: a session pushes a
fix, subscribes, ends the turn. CI finishes green.
GitHub sends the webhook; receiver drops it. The
session waits indefinitely. With poll-on-resume, the
*next* time anything wakes the session (the user
asks "how's CI?", an unrelated webhook fires, a stop
hook fires), the first action surfaces the green
status and the merge fires immediately.

If the session is also under `project supervise`,
the supervise stop-hook does the poll on its own
(see `libexec/project/supervise-stop-hook`'s
`ci_state`) — the agent reads the result in the
re-grounding payload. Same backstop, no agent-side
discipline required.

## 9. Guardrails

1. **Never enable auto-merge on a draft PR.** The
   call succeeds, but it's a process bug: drafts mean
   "not ready"; auto-merging contradicts that.
2. **Never enable auto-merge with a `changes_requested`
   review open.** Address the review first.
3. **Never auto-merge a shared-infra change** without
   an explicit user OK in the same turn.
4. **Never `--admin`-override** required checks to
   force a merge. If a check is wrong, fix the check
   or the configuration; don't bypass.
5. **Never disable auto-merge to make progress** if
   you're in the bug-fix loop (§4). Keep it armed;
   GitHub does the right thing.
6. **The post-1.0 ask-first rule has teeth.** Once
   `VERSION` crosses to `1.x.x`, the default flips:
   auto-merge becomes opt-in per PR. Don't paper
   over with a "user probably meant it" assumption.
7. **Never `sleep` waiting for CI.** See §8.

## 10. Worked example

The merge of PR #1 onto master is the worked
example. Sequence:

1. PR opened ready (not draft).
2. CI green on head (`6a72f25` → unit job success).
3. `enable_pr_auto_merge` called → failed because
   repo-level toggle was off.
4. Fallback: direct `merge_pull_request` with
   `squash` method, citing user's explicit
   authorisation ("ensure that you are automerging
   the PR once the CI is successfully executed").
5. Merge succeeded; session received the
   `Outcome: merged` event and unsubscribed.

The follow-up — flip the repo-level toggle so the
standard §3 path works for the next PR.

## 11. Cross-references

- Test-execution + CI surface: `.repo/project/skills/testing.md`
- TDD bug flow on CI red: `issues/bug/discovery.md`
- Feature delivery (this skill is its merge step):
  `issues/feature/design.md`
- Versioning + release tagging: `.repo/project/skills/version.md`
- Binding rules: `.repo/project/skills/policy/merging.md`
  → "Auto-merge gates", "Shared-infrastructure list",
  "Red CI loop policy"; `.repo/project/skills/policy/conduct.md`
  → "Hard 'never autonomously' list"
- CLAUDE.md §14 — auto-merge contract summary
