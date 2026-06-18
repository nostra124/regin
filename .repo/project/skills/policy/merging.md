# Auto-merge gates + CI interaction

> Binding rules for when autonomous merge is allowed and how to
> wait for CI results. See also `operations/automerging.md` for the
> full implementation walkthrough.

## Auto-merge gates

The agent arms auto-merge on its own PR (preferred:
`mcp__github__enable_pr_auto_merge`; fallback when the repo-level
toggle is off: `mcp__github__merge_pull_request` once CI is green)
when **all** of these are true:

1. CI status is green on the PR's head commit.
2. PR is not in `draft` state.
3. No review with `state: changes_requested` is open.
4. The PR doesn't touch any of the **shared-infrastructure**
   files listed below.
5. The target package's root `VERSION` is **pre-1.0.0** (i.e.
   `0.X.Y`). Once a package crosses to `1.0.0`, auto-merge is
   disabled by default — the user merges manually unless the
   user explicitly authorised auto-merge for that PR in the
   request.

The pre/post-1.0 rule reflects the conventional "0.x = unstable,
1.x = stable" semver semantics: pre-1.0 churn is expected and
auto-merging it keeps the loop tight; post-1.0 means the surface
is in production use and every merge deserves human eyes.

When the PR touches **multiple packages** with mixed pre/post-1.0
versions, the strictest rule wins — any post-1.0 package in the
diff disables auto-merge for the whole PR.

Full mechanics (the MCP tool sequence, the repo-level prerequisite
toggle, on-merge cleanup, how the CI-red bug-flow keeps the armed
merge in place across retries) live in **`operations/automerging.md`**.

### Shared-infrastructure list (NEVER auto-merge — ask the user)

Changes to these files require explicit user OK:

- the linter's config (e.g. `.shellcheckrc`, `.clang-tidy`,
  `pyproject.toml`, `.eslintrc`, `Cargo.toml [lints]`, etc. —
  language-dependent; see `.repo/project/skills/language/<lang>.md`)
  (repo-wide lint waivers — affects every package)
- top-level `Makefile` lint / check gate logic (affects every
  package's CI)
- `.github/workflows/*.yml` (the CI definitions themselves)
- `<pkg>/.rpk/depends/*` (dependency pinning — affects what
  installs cleanly)
- `docs/templates/{Makefile.in,configure.in,install.in,
  CLAUDE.md.foundation}` (per-package scaffolding the next
  extracted repo will inherit)
- The agent rules themselves: `<pkg>/AGENTS.md` and
  `<pkg>/.repo/project/skills/*` (changing the rules is
  a meta-change that the user should approve)

A PR that includes a non-shared-infrastructure change PLUS a
shared-infrastructure change is treated as the latter: ask first.

## Never poll for CI

After opening a PR (or pushing a follow-up commit to a subscribed
PR), the agent **ends the turn** and waits for a
`<github-webhook-activity>` event. It does **not**:

- `sleep` in Bash waiting for CI to finish.
- Loop on `pull_request_read get_check_runs`.
- Loop on `pull_request_read get_status`.
- Run any "watcher" or "babysitter" command that blocks the session.

This rule applies equally to **green** and **red** CI outcomes. The
webhook delivers both. Polling holds the session open with no
benefit, costs context, and makes the agent unresponsive to the
user.

The single exception is the same-turn verification before a
fallback direct merge (`operations/automerging.md` §5): one
`get_check_runs` call to confirm CI is green *at this moment*,
then immediately `merge_pull_request`. That is a same-moment
check, not a wait.

### Wake-up signals (and what to do when CI webhooks don't fire)

A session ending its turn after a push wakes on one of: a PR
webhook event (comment / review / merge) or the user's next prompt.
**CI-completion webhooks are not reliably delivered to subscribed
sessions on many host configurations** — only the post-merge event
consistently arrives.

The binding rule:

- **Repo with auto-merge enabled** (Settings → General → Pull
  Requests → Allow auto-merge ON): `enable_pr_auto_merge` is called
  once after opening the PR. GitHub merges on its own; no session
  involvement needed until the merge webhook arrives.
- **Repo with auto-merge disabled** (default): the agent follows the
  **fallback path** (`operations/automerging.md` §5). On *any* session
  wake after a push — whether a webhook or a user prompt — the agent
  runs one same-turn `get_check_runs` call and acts on the result.
  The session never polls; it acts opportunistically when woken for
  any reason.

This is the procedure new projects should use until they flip the
repo-level toggle. It works without any host-specific configuration.

## Red CI loop policy

| Attempt | Action |
|---|---|
| 1st red | investigate root cause via the PR-comment failure log; file `BUG-NNN`; write failing test (TDD per `issues/bug/discovery.md`); fix; push new commit; **end the turn** |
| 2nd red | same; the fix is converging |
| 3rd red | same |
| 4th red | **pause and ask the user** — the failure is probably architectural |

Between attempts the session waits for a
`<github-webhook-activity>` event — no polling, no sleep
(see "Never poll for CI" above).

Each fix attempt is a **new commit**, never `--amend`. The PR
history shows the iteration; reviewers can see what the agent
tried. If auto-merge was armed on the PR, it **stays armed**
across the retry loop; GitHub will fire it the moment a green CI
lands.

## Review-comments policy

| Comment kind | Action |
|---|---|
| Confident + small + non-architectural | apply the fix; push; comment back briefly explaining what changed |
| Ambiguous / could be interpreted multiple ways | use `AskUserQuestion` (or equivalent) to disambiguate; pause until answered |
| Architecturally significant (touches shared infrastructure, design assumption, public API) | pause + ask before any change |
| Duplicate of an earlier comment / outdated | reply briefly explaining; skip the change |
| No-action (nit, opinion, +1) | acknowledge; no commit |