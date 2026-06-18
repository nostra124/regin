# Phase-transition gates

> Binding rules enforced by `project transition <id> <phase>`.
> Compare with `methodology/vmodel.md` (phase model) and
> `convention/tickets.md` (frontmatter schema).

Every ticket carries a `phase:` frontmatter field — one of
`open / design / build / test / done`. Transitions are mechanically
enforced against the following binding gates:

| Transition | Gate (must hold) |
|---|---|
| open → design (features only) | none — Design session has been started; ticket pulled from a planning Design |
| design → build | `## Design` section present in ticket body **and** no production code committed against the ticket yet |
| build → test | unit suite green **and** at least one `tests/unit/*` change committed on the ticket's branch |
| test → done | SIT green (and PIT green if scoped per "Test coverage matrix") **and** PR merged into the appropriate target (master or integration branch) |
| open → build (bugs only) | none — bugs skip the Design phase per `issues/bug/discovery.md` §1 |

**Hard-stops:**

- A bug **must not** transition into `design`. If design work is
  required, file a new FEAT (escape hatch — `issues/bug/discovery.md` §1).
- A ticket **must not** transition to `done` without a corresponding
  merged PR. The phase change and the merge happen in the same
  movement; ordering is "PR merged → run `project transition <id>
  done`".

**Backwards transitions** (e.g. `build → design` when implementation
reveals a design hole) are allowed and recorded — the agent appends a
new line to `<id>.sessions.jsonl` noting the backwards step. High
backwards-transition rates per size-class indicate the sizing rubric
is mis-calibrated, surfaced by effort roll-ups.

Phases apply from M2.2.0 onwards; M2.1.0 and earlier tickets are
grandfathered (`.repo/project/skills/issues/feature/design.md` §7).