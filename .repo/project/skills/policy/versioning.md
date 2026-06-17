# Versioning + commit hygiene

> Binding rules for VERSION bumps and commit discipline. See also
> `.repo/project/skills/version.md` for the full bump checklist.

## Versioning

The umbrella semver lives in a **`VERSION`** file at the repo root.
Single source of truth, read by `bin/<pkg>` at runtime, validated by
`./configure`, installed to `$prefix/share/<pkg>/version` by
`make install`. See **`.repo/project/skills/version.md`** for the
full bump checklist.

Binding rules:

| Bump  | Trigger                                              |
|-------|------------------------------------------------------|
| MAJOR | Breaking change to a documented verb / flag / path.  |
| MINOR | New verb / flag / sub-service added; no removal.     |
| PATCH | Bug fix; no surface change.                          |

Refactors, doc edits, and internal-only changes do not move the
version. The pin in the version-pin unit test
(`tests/unit/<pkg>`, language-specific extension — see
`.repo/project/skills/language/<lang>.md`) gates this: no test
change ⇒ no bump.

The full checklist (one-PR rule, test-pin sync, `make package
VERSION=…` release flow) lives in **`.repo/project/skills/version.md`**.

## Commit hygiene on master

When working directly on `master` (no feature branch), the agent
MUST commit after each logical unit of work. A logical unit is:

| Unit | Examples |
|---|---|
| A filed or updated ticket | creating a DISC/FEAT/BUG, advancing its phase, adding spawned features |
| A completed code change | fixing a bug, adding a verb, refactoring a function |
| A completed doc change | updating help text, rewriting a README, editing policies |
| A version bump | `VERSION` + `.rpk/versions` + tag |

**Hard-stop** — accumulating multiple logical units in one
uncommitted batch on `master` is a policy violation. The agent
commits after each unit, pushes to the bare repo, and syncs the
staged worktree before starting the next unit.

Rationale: small commits on master make `rpk package` reproducible,
make bisect meaningful, and ensure the staged worktree never drifts
far from the commit tree.

## Logging

Every script or binary in this project MUST expose
`debug / info / warn / error` plus `fatal / die`,
following the canonical contract in
**`.repo/project/skills/logging.md`**. Concrete helper
implementations are language-dependent — see
`.repo/project/skills/language/<lang>.md`.

Binding rules:

| Stream  | Goes to | Audience                             |
|---------|---------|--------------------------------------|
| stdout  | data    | pipeline consumers, scripts          |
| stderr  | progress, warnings, errors, fatal | humans, CI logs |

`info` is suppressed by `$SELF_QUIET`; `debug` is gated by
`$SELF_DEBUG`; `warn` and `error` always print. **No ANSI codes**
in `error` / `warn` / `fatal` — CI captures these into PR comments
and colour escapes render as garbage there.

A binary that doesn't expose all four levels (or that mixes data
into stderr / chatter into stdout) is non-conformant; flag it as a
follow-up bug.