# Agent loop + local-first execution

> The autonomous loop the agent follows for each ticket. See also
> `policy/merging.md` for auto-merge mechanics and `policy/testing.md`
> for the test coverage matrix.

## Agent loop (autonomous)

```
1. read ticket    →  issues/feature/<phase>/<num>-*.md
2. plan + code    →  branch claude/<feat-id>-<slug>
3. write tests    →  unit ALWAYS; SIT for features; PIT for external
                     integration (see policy/testing.md)
4. local gates    →  fail-fast; run all that are runnable locally:
                       make lint
                       make check          (unit + vectors)
                       make check-sit      (run if podman; soft-skip
                                            with explicit PR-body
                                            note if missing)
                       make check-pit      (run only if podman +
                                            credentials present)
                       make coverage       (informational; if kcov
                                            present)
5. all green     →   open PR ready (not draft)
6. subscribe     →   mcp__github__subscribe_pr_activity
7. end turn      →   do NOT poll; do NOT sleep; do NOT loop on
                     get_check_runs / get_status. Webhook events
                     wake the session for both green and red CI
                     outcomes (see operations/automerging.md §8).
8. CI green      →   arm auto-merge if pre-1.0 (per policy/merging.md);
                     ask user otherwise. On merge: move ticket →
                     issues/feature/done/; append Resolution
                     section (see operations/automerging.md §7).
9. CI red         →   investigate via the PR-comment failure log
                     (skills/testing.md §3); switch to the bug
                     flow (issues/bug/discovery.md); fix → push → end turn
                     again. After N retries (default 3), pause
                     and ask the user.
 10. review comments → see policy/conduct.md "Review-comments policy".
 11. session retro  → write `retro/YYYY-MM-DD-<slug>.md` capturing
                      what surprised me, policy gaps, filed tickets,
                      and next-time changes (see operations/retrospective.md).
                      Binding policy — MUST do before ending the
                      session when any artefact was produced (PR,
                      ticket, commit, discovery finding). Cross-
                      project findings get a ticket in the affected
                      repo, not here.
```

**CI is expensive — local first.** Local execution covers
`unit + vectors + SIT (podman available) + PIT (podman + credentials
available)`. CI is the safety net, not the discovery mechanism.

If a tier is locally skipped (e.g. podman missing), the PR body
**explicitly states which tier was skipped and why**. That makes
the gap auditable.

## Supervised milestone execution

For multi-ticket milestones, the agent loop runs end-to-end inside
a single session via **`project supervise <milestone>`**. The verb
registers a session-scoped **Stop hook** that refuses premature
"I'm done" turns, re-grounds the agent on every fire, and
**checkpoints at each ticket boundary** so the user can pause
between tickets without losing context. CI waits use the existing
`subscribe_pr_activity` sleep — no polling. See
`operations/milestone.md` → supervised execution for the full
protocol; `project supervise --status` introspects the current
session's state without invoking the LLM.