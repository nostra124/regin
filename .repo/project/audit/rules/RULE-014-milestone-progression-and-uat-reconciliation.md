# RULE-014 — Milestone progression and UAT-vs-roadmap reconciliation

scope: full
severity: warn
gate_of: dev-pipeline

## Rule

Every dvalin-supervised project follows a fixed early-milestone shape, ships a
UAT-able artefact at the end of **every** milestone, and reconciles UAT findings
against the roadmap before filing them.

1. **0.1.0 — roadmap and design.** For a fresh project, milestone 0.1.0 derives
   the **roadmap** (the ordered milestone plan) and the **design discussions**
   (DISC tickets resolved into decisions). It produces no shippable code — its
   deliverable is the plan everything else is measured against.
2. **0.2.0 — first deployable version.** Milestone 0.2.0 delivers the first
   working, **deployable** version: a complete thin slice with native packages,
   an install path, a deploy script, and baseline documentation — not a feature
   spike. From 0.2.0 onward there is always something a user can install.
3. **Every milestone ends shippable and UAT-able.** When a milestone reaches
   `stable`, the project is shippable *and* the packaged artefact is handed to
   UAT. Defects and breakouts found during UAT are captured as BUG tickets in
   **patch milestones** (`0.x.y`, `z > 0`) off that milestone — never by
   reopening the closed feature milestone.
   - **Install and demo from the native package.** In alpha, beta, and the
     end-of-milestone demo, every install is done from the built native package
     (`.deb`/`.rpm`/`.apk`/`.pkg`) — never `cargo install`, `make install`, or a
     hand-copied binary. The milestone demo is driven through the installed
     binaries; if it cannot run from the package, the milestone is not done.
4. **Reconcile every UAT finding with the roadmap.** Before a UAT finding
   becomes a bug, check it against the existing roadmap. If it describes work
   already planned for a **later** milestone, do not silently file a full bug:
   decide, with the user, whether to (a) raise a **partial** bug for the part
   that is genuinely broken now, or (b) defer to the planned milestone. Record
   the decision on the finding.

## Pass criteria

- A fresh project's 0.1.0 contains the roadmap + resolved DISCs and no product
  code; 0.2.0 delivers a deployable, packaged, documented slice.
- Each `stable` milestone has a shippable, UAT-handed artefact; UAT defects live
  in patch milestones, not the reopened feature milestone.
- Alpha/beta installs and the end-of-milestone demo run from the installed
  native package, not from source or a hand-copied binary.
- UAT-derived BUG tickets cite the roadmap reconciliation: either "not on the
  roadmap" or an explicit partial-bug / defer decision for roadmap items.

## Fail criteria

- 0.1.0 ships product code, or skips the roadmap/design deliverable.
- 0.2.0 is a feature spike with no deployable/packaged/documented output.
- A milestone is declared stable but cannot be installed or handed to UAT.
- Alpha/beta or the milestone demo is run from `cargo`/source/a hand-copied
  binary instead of the installed native package.
- A UAT finding that restates planned later-milestone work is filed as a full
  bug (or silently dropped) without a recorded reconciliation decision.

## Audit instruction

1. Confirm the roadmap exists and that 0.1.0 is design/roadmap-only and 0.2.0 is
   the first deployable, packaged, documented version. Report PASS/FAIL.
2. For each closed feature milestone, verify a shippable artefact exists and that
   UAT-found defects were filed in patch milestones (`0.x.y`), not by reopening
   the milestone.
3. For each UAT-derived BUG ticket, verify it records a roadmap reconciliation:
   either it is genuinely new, or it carries an explicit partial-bug-vs-defer
   decision for work already on the roadmap. Flag any that do not.
