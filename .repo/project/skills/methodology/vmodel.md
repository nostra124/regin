# V-Model + agile/CI overlay

> The process model every feature flows through. Compare with
> `policy/transitions.md` (binding gates) and
> `convention/tickets.md` (frontmatter schema).

## Phases

Every feature flows through four phases:

```
     ┌──────────┐                            ┌──────────┐
     │ Design   │ ──────────────────────────▶│ Deploy   │
     └──────────┘                            └──────────┘
          │                                       ▲
          ▼                                       │
     ┌──────────┐                            ┌──────────┐
     │ Build +  │ ─── unit ──── SIT ─── PIT ─▶│ Test     │
     │ Code     │                            │          │
     └──────────┘                            └──────────┘
```

| Phase | Artefact | Where | Cadence |
|---|---|---|---|
| **Open** | ticket file | `issues/{feature,bug}/<phase>/<num>-*.md` (frontmatter + AS-A description + Acceptance Criteria) | filed before any work |
| **Design** | `## Design` section + sizing + estimates | filled into the ticket | features only; bugs skip this phase |
| **Build** | code + test | one PR per ticket | when AC are clear (features) / immediately (bugs) |
| **Test (unit)** | unit suite per package (runner is language-specific; see `.repo/project/skills/language/<lang>.md`) | `tests/unit/*` | every push |
| **Test (SIT)** | podman fixture | `tests/sit/podman/Dockerfile.<pkg>` + suites | every push, after unit |
| **Test (PIT)** | real-environment suite | `tests/pit/suites/*` | nightly only |
| **Done** | merged PR + ticket → `done/` | issue file moved + frontmatter `status: done` + `phase: done` + Resolution section | on merge |

The current phase is a **per-ticket `phase:` frontmatter field** —
`open` / `design` / `build` / `test` / `done`. Transitions are
mechanically enforced by `project transition <id> <phase>` against
the binding gates in `policy/transitions.md`.

**Bugs skip Design** — their lifecycle is Open → Build → Test → Done.
If a bug investigation surfaces a design issue, file a new FEAT
(`issues/bug/discovery.md` → "Design-issue escape hatch").

## One feature → one PR

Each feature is exactly one ticket and exactly one merge commit.

PR title:
```
<TICKET-ID>: <one-liner>
```

Examples that match this collection's existing history:
```
FEAT-208: make coverage (kcov) + CI artifact upload
FEAT-149/150: absorb bin/{mailfilter,api} → user/libexec/user/
FEAT-209 (1/2): surface current lint warning inventory
```

## Bug ↔ feature ↔ semver routing

| Change kind | Bump | Filename pattern |
|---|---|---|
| **Bug fix** | patch (`X.Y.0` → `X.Y.5`, half-step) | `MILESTONE-X.Y.5.md` |
| **Additive feature** | minor (`X.Y.0` → `X.(Y+1).0`) | `MILESTONE-X.(Y+1).0.md` |
| **Breaking surface change** | major (`X.0.0` → `(X+1).0.0`) | `MILESTONE-(X+1).0.0.md` |

This collection uses the **half-step** convention for patches
(`0.X.5` not `0.X.1`). The slot may also carry small additive
features that don't warrant a full minor bump.

**Bugs take precedence over features at the same priority.** A
`critical` bug interrupts whatever feature work is in flight; a
`medium` bug ships before a `medium` feature in the same release.

## Per-project extension hook

A project may add stricter requirements via its own `AGENTS.md`
(e.g. `bitcoin` mandates regtest SIT for any wallet-touching
feature). The general rules above are the **floor**; a project
can raise it but cannot lower it.