# Per-package file layout

> Structural conventions for the directory tree. Reviewed for fit;
> not gate-enforced.

Every package follows:

```
<pkg>/
  README.md                       per-package readme
  CLAUDE.md                       per-repo agent guide
  AGENTS.md                       per-repo agent rules (delta from canonical project rules)
  VERSION                         canonical umbrella semver (one line); see .repo/project/skills/version.md
  retro/                          session retrospective entries (per-step 11 of agent loop)
  bin/<pkg>                       entry point
  libexec/<pkg>/<verb>            sub-commands (where applicable)
  share/man/man1/<pkg>.1          man page
  share/doc/<pkg>/standards/      vendored standards refs
  .repo/project/skills/<topic>.md implementation walkthrough (testing, version, logging, bugs,
  |                               features, automerging, milestones, audit, discovery, retro, …)
  tests/unit/<pkg>                unit tests (language-specific extension — see
  |                                 .repo/project/skills/language/<lang>.md)
  tests/sit/podman/Dockerfile.<target>  SIT fixture per concern (e.g. Dockerfile.tmux)
  tests/pit/suites/                PIT (when present)
  (shell-completion file is optional and language-dependent;
   see .repo/project/skills/language/<lang>.md)
  hooks/pre-push                  unit always; sit+pit when podman present
  .github/workflows/test.yml      CI: unit on every PR; failure → PR comment
  docs/<pkg>.md                   CLI reference
  docs/<pkg>-walkthrough.md       end-to-end example
  docs/task.md                    task sub-service reference (when present)
  issues/feature/<phase>/         per-package feature tickets (open/design/build/test/done)
  issues/bug/<phase>/             per-package bug tickets (open/build/test/done)
  issues/discovery/<phase>/       discovery tickets (describe/ideate/done)
  issues/BACKLOG.md                unassigned pool
  issues/MILESTONE-<x>.<y>.<z>.md   per-milestone plan
  Makefile.in / configure / install   autoconf umbrella (configure reads VERSION)
  Makefile                        committed dev wrapper (overwritten on ./configure)
  .gitignore                      autoconf-generated Makefile + build/
  .rpk/versions                   TSV ledger (append-only history; VERSION is canonical)
  .rpk/depends/<dep>              dependency markers
```

A per-project `AGENTS.md` MAY exist alongside `CLAUDE.md`; it
lists the **delta** from the canonical project rules (see
`AGENTS.md` → "Project-specific extension").

## Where each policy / convention lives

- **Test coverage requirements** (bug → unit; feature → unit + SIT;
  external integration → +PIT) — `policy/testing.md`.
- **Phase-transition gates** (open → design → build → test → done;
  bug shortcut; backwards-transition rule) — `policy/transitions.md`.
- **Local-first execution rules** — `policy/testing.md`.
- **Auto-merge gates / hard-stop conditions** — `policy/merging.md`.
- **Branch / commit / PR title shape / Session: trailer** —
  `convention/naming.md`.
- **Ticket frontmatter** (including `phase`, `complexity`,
  `estimate_tokens`, `estimate_time`)**, file naming,
  MILESTONE-<x>.<y>.<z>.md conventions, `.sessions.jsonl` schema** —
  `convention/tickets.md`.
- **Per-language idiom** — `.repo/project/skills/language/<lang>.md`.
- **Testing protocol implementation** (pre-push hook,
  `.github/workflows/test.yml`, where unit / SIT / PIT each
  run, how CI failures land in PR comments) —
  `.repo/project/skills/testing.md`.
- **Version bumping** (root `VERSION` file, semver rules,
  bump checklist, the test pin contract) —
  `.repo/project/skills/version.md`.
- **Logging contract** (debug / info / warn / error +
  fatal / die; stderr discipline; CI-readability rules)
  — `.repo/project/skills/logging.md`.
- **Bug-handling (TDD)** (file ticket → write failing
  test → commit → fix → commit → done; CI-failure
  loop; design-issue escape hatch) —
  `.repo/project/skills/issues/bug/discovery.md`.
- **Feature delivery walkthrough** (V-model phases;
  sizing rubric; ticket → branch → code + tests →
  local gates → ready PR → subscribe → auto-merge →
  move to `done/`) —
  `.repo/project/skills/issues/feature/design.md`.
- **Auto-merging** (gates, MCP tool sequence, repo-
  level prerequisite, CI-red bug-flow interaction,
  fallback path) —
  `.repo/project/skills/operations/automerging.md`.
- **Milestones** (per-version `MILESTONE-<x>.<y>.<z>.md`
  files, ticket assignment, shuffling protocol,
  delete-on-close, per-phase multi-ticket session
  rules, effort-estimate roll-up) —
  `.repo/project/skills/operations/milestone.md`.
- **Traceability audit** (every shipped surface ⇄
  at least one ticket; orphan + phantom + stale
  classification; advisory output only) —
  `.repo/project/skills/operations/audit.md`.

## License

GPL-3 across the collection. Per-repo `LICENSE` file ships with
the extracted repository at v0.20.0 extraction time.