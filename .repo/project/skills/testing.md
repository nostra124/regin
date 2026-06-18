---
name: testing
description: |
  Run the right test layer for the current host. Cloud
  sandboxes run only unit tests; desktops with podman
  run unit + SIT + PIT. CI in GitHub Actions runs
  unit and posts any failure log as a PR comment so
  agents subscribed to the PR can read it. Trigger
  when about to push, when investigating a CI failure,
  or when adding tests.
---

# `testing` skill

## 1. Three test layers

| Layer | Path           | Runner             | Requires                    |
|-------|----------------|--------------------|-----------------------------|
| Unit  | `tests/unit/`  | `make check-unit`  | language test runner        |
| SIT   | `tests/sit/`   | `make check-sit`   | language test runner, `podman` |
| PIT   | `tests/pit/`   | `make check-pit`   | language test runner, `podman` |

- **Unit** — pure in-process, no external services. Must
  run green on every host the binary runs on. The
  concrete test runner is language-dependent; see
  `.repo/project/skills/language/<lang>.md`.
- **SIT** — System Integration Tests. Per-backend
  containers (autoconf / cmake / meson / cargo / npm /
  go / pyproject / deb / rpm / pkg / apk).
- **PIT** — Production Integration Tests. Reserved for
  multi-host scenarios that exercise the umbrella
  against real installs. Not yet populated.

## 2. When to run which

| Environment              | Test layers to run          |
|--------------------------|-----------------------------|
| Cloud sandbox            | unit                        |
| Desktop with podman      | unit + sit + pit            |
| Desktop without podman   | unit                        |
| GitHub Actions (CI)      | unit (matrix expands later) |

The pre-push hook (`hooks/pre-push`, installed via
`make install-hooks`) enforces this: it always runs unit,
and adds SIT + PIT when `command -v podman` succeeds.

## 3. CI in GitHub Actions

`.github/workflows/test.yml` runs `make check-unit` on
every `pull_request` and on pushes to `master` / `main`.
On failure it posts the trimmed test log (last 400
lines) as a PR comment, including a link to the workflow
run.

Permissions required on the workflow:

    permissions:
      contents: read
      pull-requests: write

Agents subscribed to PR activity receive the failure
comment as a `<github-webhook-activity>` event, so the
test output is directly readable in the conversation
without scraping CI logs.

**Webhook events wake the session for both green and
red CI outcomes.** After pushing a commit, the agent
ends the turn and waits for the event — it does not
poll, sleep, or run a watcher. CI runtime varies; the
session does not need to be alive during the run. See
`operations/automerging.md` §8 for the full rule.

## 4. Workflow recipes

1. **Before any push from this repo.**

       make install-hooks       # one-time
       # subsequent pushes auto-run pre-push

2. **Run just the unit layer manually.**

       make check-unit

3. **Run the SIT matrix on a podman-capable host.**

       make check-sit           # soft-skips without podman

4. **Run everything before a release.**

       make check-all           # unit + sit + pit

5. **Reproduce a CI failure locally.**

       make check-unit          # same command CI runs

## 5. Guardrails

1. **Never bypass the pre-push hook** with `--no-verify`
   unless you know exactly why. Hook failures point at
   real regressions; fix the test or the code, don't
   skip.
2. **Don't bundle SIT into pre-commit.** Unit tests are
   cheap (< 5s); SIT spins containers. Hook runs SIT
   only at push time, not commit time.
3. **Soft-skip, don't hard-fail, on missing tooling.**
   `make check-sit` on a host without podman should
   exit 0 with a "soft-skipping" note, not error. The
   hook follows the same rule.
4. **The PR comment is for the agent's benefit.** Don't
   strip ANSI codes or reformat the log — Claude's PR
   activity subscription wants the raw test output.

## 6. Where to read more

- `hooks/pre-push`                              — the hook itself
- `Makefile.in` → `check-unit / check-sit / check-pit / install-hooks`
- `.github/workflows/test.yml`                  — CI workflow
- `.repo/project/skills/language/<lang>.md`     — concrete test runner + file conventions
- CLAUDE.md § 10 — the protocol summary
