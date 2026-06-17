# RULE-010 — Phase entry criteria for alpha, beta, and stable

scope: full
severity: block
gate_of: dev-pipeline

## Rule

Each release phase has mandatory entry criteria that must be fully satisfied
before promotion.  Promoting a milestone without meeting these criteria is a
process violation.

The phases are:

```
open → alpha → beta → stable
```

**`open`** is the planning phase — tickets are being written and sequenced.
**`alpha`** is single-user acceptance.  **`beta`** is multi-user acceptance.
**`stable`** is general availability.

---

### Milestone planning — prerequisites before alpha can start

Before a milestone may transition from `open` to `alpha`, four delivery
capabilities must be **planned and ticketed** inside the milestone.  They do
not need to be complete yet, but each must have a FEAT ticket in the
milestone's `tickets:` list and a clear scope.  A milestone plan that is
missing any of these is **incomplete** and must not enter alpha.

| Capability | Requirement |
|---|---|
| **100% test coverage** | A FEAT ticket exists that commits to 100% unit + SIT + PIT coverage for all modules shipping in this milestone. |
| **Platform packages** | A FEAT ticket exists for building and uploading **native packages** (`.deb`/`.rpm`/`.apk`/macOS `.pkg`/Homebrew — per `profile.md` §7), each shipping every binary the milestone produces plus its service units, for every supported platform (including mobile targets if defined) to the GitHub release. A binary tarball alone does not satisfy this. |
| **Install script** | A FEAT ticket exists for `install.sh` (or equivalent) that is exercised end-to-end in the PIT layer. |
| **GitHub wiki** | A FEAT ticket exists for a wiki landing page: one-paragraph description, install instructions, and quick-start content current with this milestone. |

These tickets are reviewed and confirmed during the **milestone kickoff** (see
`skills/operations/milestone.md` §5 and `dvalin milestone kickoff`).  If any
are missing at kickoff, they must be filed before any implementation work begins.

---

### Alpha — single-user acceptance test

Alpha is the phase where the software is feature-complete and fully verified
by automated test suites.  It is handed to a **single internal user** for
acceptance testing.

**Entry criteria (all must be met):**

1. Every FEAT ticket in the milestone's `tickets:` list is in `done/` —
   including the four planning prerequisites above.
2. Unit tests pass for all modules with **100% test coverage** (RULE-001).
3. System Integration Tests (SIT) pass (RULE-002).
4. Process Interaction Tests (PIT) pass inside a podman container (RULE-003,
   RULE-004).
5. No open design questions remain in any FEAT ticket (RULE-005).
6. No stub implementations remain (RULE-006).
7. No open BUG tickets targeting this milestone.
8. **Native packages** for **all supported platforms** are built and uploaded
   to the GitHub release (or pre-release) for this version tag — `.deb`, `.rpm`,
   `.apk`, macOS `.pkg`, Homebrew, per the platform list in `profile.md` §7.
   Each package must ship **every** binary the milestone produces plus its
   service units; a binary tarball alone does **not** satisfy this criterion.
   Mobile targets are included if defined by the project.
9. An **install script** (`install.sh` or equivalent) exists at the repo root
   and is tested end-to-end in the PIT layer.
10. The **GitHub wiki landing page** exists with: project description, install
    instructions (referencing the install script), and a quick-start section,
    all current with this milestone's feature set.
11. The audit for the milestone has been completed and filed in `audit/done/`.

### Beta — multi-user acceptance test

Beta is handed to **multiple external or representative users** for acceptance
testing.  The software must have already passed alpha.

**Entry criteria (all must be met):**

1. All alpha entry criteria above are satisfied.
2. Alpha acceptance test feedback has been reviewed and any blocking issues
   filed as BUG tickets in a patch milestone and resolved.
3. At least one complete alpha acceptance test cycle has been run by a single
   user and signed off.

### Stable — general availability

Stable marks the milestone as ready for general release.

**Entry criteria (all must be met):**

1. All beta entry criteria above are satisfied.
2. Beta acceptance test feedback has been reviewed and any blocking issues
   resolved.
3. At least one complete beta acceptance test cycle has been run with multiple
   users and signed off.
4. Release notes (or changelog entry) are written.
5. The git tag `v<version>` exists on the release commit.

## Phase promotion sequence

```
open → alpha → beta → stable
```

Skipping a phase (e.g. alpha → stable directly) is a block violation unless
the milestone is explicitly marked `single-phase: true` in its frontmatter
and the project owner has approved the skip in the milestone file.

## Pass criteria

- The milestone frontmatter `phase:` value matches the entry criteria
  evidence above.
- Every criterion for the current phase is satisfied before promotion is
  attempted.
- Before `open → alpha`: the four planning prerequisites are each covered by
  a ticket in `tickets:`.
- AUDT ticket exists in `audit/done/` before alpha promotion.
- Beta entry requires documented alpha sign-off (a comment or closing note
  in the milestone file or a linked issue).
- Stable entry requires documented beta sign-off.

## Fail criteria

- Milestone transitions `open → alpha` but any of the four planning
  prerequisites (coverage, packages, install script, wiki) has no ticket in
  `tickets:`.
- `phase: alpha` but test coverage is below 100%.
- `phase: alpha` but release packages are missing for one or more supported
  platforms (including defined mobile targets).
- `phase: alpha` but no install script exists or PIT does not test it.
- `phase: alpha` but the GitHub wiki landing page is absent or out of date.
- `phase: beta` but unresolved BUG tickets from alpha remain open.
- `phase: stable` but no git tag `v<version>` exists.
- Any phase set without all FEAT tickets in `done/`.
- Any phase set without passing SIT and PIT results on record.
- Phase promotion skipped without `single-phase: true` approval.

## Audit instruction

### Planning audit (open → alpha gate)

1. Read the milestone `tickets:` list.
2. Verify a ticket exists for each of the four planning prerequisites:
   - 100% test coverage commitment
   - Platform packages + GitHub release upload
   - Install script (PIT-tested)
   - GitHub wiki landing page
3. If any ticket is missing, the milestone plan is incomplete — FAIL.

### Alpha promotion audit

1. Verify all FEAT tickets in `tickets:` are in `done/`.
2. Confirm CI logs or test run evidence exists for unit, SIT, and PIT.
3. Verify test coverage report shows 100% for all modules.
4. Confirm **native packages** (`.deb`/`.rpm`/`.apk`/macOS `.pkg`/Homebrew)
   exist on the GitHub release/pre-release for every supported platform listed
   in the project profile, and that each one ships every binary the milestone
   produces plus its service units (a tarball alone fails this check).  If
   mobile targets are defined, verify those artefacts are present.
5. Verify `install.sh` (or equivalent) exists and is exercised in PIT.
6. Verify the GitHub wiki has a landing page with description, install
   instructions, and quick-start content current with this milestone.
7. Check `audit/done/` for a completed AUDT ticket for this milestone.
8. Confirm no open BUG tickets target this milestone version.

### Beta and stable promotion audit

9. For beta: look for alpha sign-off note in the milestone file or a linked
   discovery/issue.
10. For stable: run `git tag -l v<version>` and confirm the tag exists.
11. For any promotion: confirm no open BUG tickets target this milestone
    version.
