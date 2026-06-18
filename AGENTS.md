# AGENTS.md — agent bootstrap and methodology gateway

This file is the single entry point for any coding agent working in this
repository. Read it top-to-bottom before touching any code.

This file is **managed by the toolchain and overwritten on every update**.
Do not add project-specific content here. See § Storage locations below.

---

## Storage locations

Three places for content — use the right one:

| Location | Owner | What goes here |
|---|---|---|
| `.repo/dvalin/` | agents | Anything only the coding agent needs: session notes, decisions, quirks, deferred items, project-specific instructions. The workflow engine (dvalin) owns this dir in every repo it manages. Never overwritten by the toolchain. |
| `docs/` | project team | General documentation for humans and agents alike: architecture, ADRs, runbooks, onboarding. |
| `issues/` | project team + agents | All tracked work: FEAT, BUG, DISC, AUDT tickets and milestones. |

Start every session by reading `.repo/dvalin/notes.md` — it contains what
previous agents learned about this project.

---

## Taxonomy

| Term | Meaning | Enforcement |
|---|---|---|
| **methodology** | Overall process model — V-Model phases, agent loop, milestone discipline | Descriptive; one document |
| **policy** | Binding rule the agent MUST follow | Hard-stop; violations block PRs |
| **convention** | Naming, structural, or layout choice | Reviewed; not gate-enforced |
| **guideline** | Recommendation; idiom-focused | Soft; overrideable with reason |

---

## Ordered reading list

Read in order before starting any work. Each entry states why it matters.

| # | File | Why |
|---|---|---|
| 1 | [`.repo/project/profile.md`](.repo/project/profile.md) | What this project IS — scope, language, build system, dependencies. Everything else assumes this. |
| 2 | [`.repo/dvalin/notes.md`](.repo/dvalin/notes.md) | What previous agents learned — decisions, quirks, constraints, open questions. Read before any code. |
| 3 | [`.repo/project/skills/methodology/vmodel.md`](.repo/project/skills/methodology/vmodel.md) | The V-Model: phases, agent loop, one-ticket-one-session, semver routing. The core mental model. |
| 3a | [`.repo/project/skills/methodology/discovery.md`](.repo/project/skills/methodology/discovery.md) | How new work enters the system: DISC tickets → FEAT tickets → milestone. Read before filing any ticket. |
| 4 | [`.repo/project/skills/policy/testing.md`](.repo/project/skills/policy/testing.md) | Binding rules: test coverage matrix, auto-merge gates, CI hard-stops, never-poll-for-CI. |
| 5 | [`.repo/project/skills/policy/transitions.md`](.repo/project/skills/policy/transitions.md) | Phase-transition gates — entry and exit criteria for each milestone phase. |
| 6 | [`.repo/project/skills/convention/tickets.md`](.repo/project/skills/convention/tickets.md) | Ticket naming, file layout, frontmatter schema, branch and commit conventions. |
| 7 | [`.repo/project/skills/language/<lang>.md`](.repo/project/skills/language/) | Idioms for this project's language — lint, test runner, style. Check `profile.md` for which language. |
| 8 | [`.repo/project/skills/issues/feature/design.md`](.repo/project/skills/issues/feature/design.md) | Feature design phase — sizing, scoping, failing-test-first. |
| 9 | [`.repo/project/skills/issues/feature/build.md`](.repo/project/skills/issues/feature/build.md) | Feature build phase — implementation, test green, PR gates. |
| 10 | [`.repo/project/skills/issues/bug/build.md`](.repo/project/skills/issues/bug/build.md) | Bug build phase — TDD: red → green → fix → commit. |
| 11 | [`.repo/project/skills/operations/milestone.md`](.repo/project/skills/operations/milestone.md) | Milestone planning — MILESTONE-X.Y.Z.md structure, DISC-driven feature intake, supervised execution. |
| 12 | [`.repo/project/skills/operations/automerging.md`](.repo/project/skills/operations/automerging.md) | Auto-merge gates and the no-poll CI-wait pattern. |
| 13 | [`.repo/project/skills/operations/audit.md`](.repo/project/skills/operations/audit.md) | Traceability audit — every shipped function maps back to a ticket. |
| 14 | [`.repo/project/skills/operations/retrospective.md`](.repo/project/skills/operations/retrospective.md) | Session retrospective — what to capture, when, and how to file tickets from findings. |

---

## Session discipline

Every session follows this structure — no exceptions:

1. Read this file and `.repo/dvalin/notes.md`.
2. Identify the active ticket (the highest-priority open ticket in
   `.repo/project/issues/`, bugs before features at equal priority).
3. Work **one ticket to done/** per session. Do not touch scope outside the ticket.
4. Before ending the session, append any new findings to `.repo/dvalin/notes.md`.
5. Move the completed ticket to `done/`.

If you discover new work during a session: file a ticket immediately, then
decide — block on it or defer it. Never address untracked work silently.

---

## Skills tree

All documents under `.repo/project/skills/` are managed by the toolchain and
overwritten on update. Do not edit them directly — they are the canonical
methodology reference for this project.

```
.repo/project/skills/
├── methodology/      vmodel, agent-loop, discovery
├── policy/           testing, transitions, merging, versioning, conduct
├── convention/       tickets, naming, layout
├── operations/       milestone, audit, automerging, retrospective
├── language/         rust, shell, go, python, javascript, cpp
├── issues/
│   ├── feature/      discovery, design, build, test, release
│   └── bug/          discovery, design, build, test, release
├── project-author.md
├── project-reviewer.md
├── project-auditor.md
├── project-packager.md
├── project-troubleshooter.md
├── logging.md
├── testing.md
└── version.md
```
