# Check decomposition map (FEAT-084, DISC-021)

The monolithic `check` (CHK-001..043) dismantled into **where each check belongs**.
Categories:

- **entry** — a precondition gate before a step/phase (esp. the milestone-cycle
  feature audit + promotion gates).
- **exit** — an acceptance gate after a step (the per-step bar).
- **rule** — a rule the *dwarf follows during the work*, so it's right the first
  time (folded into the developer role, FEAT-088).
- **gate** — an end-of-workflow / milestone gate step (ticket + milestone hygiene,
  run by the conductor at milestone boundaries).

All current checks are **deterministic** (static analysis), so all become
**scripts** (FEAT-085); the `rule` ones are *also* stated to the dwarf.

| CHK | Name | Category | Det |
|-----|------|----------|-----|
| 001 | CargoTest | exit | ✓ |
| 002 | ClippyClean | exit | ✓ |
| 003 | NoPlaceholders | exit + rule | ✓ |
| 004 | NoTodoFixme | rule (+exit) | ✓ |
| 005 | MilestoneTitleMatchesFilename | gate | ✓ |
| 006 | DoneExitCriteria | gate | ✓ |
| 007 | TicketsHaveTarget | gate | ✓ |
| 008 | VersionMatchesPhase | gate (release) | ✓ |
| 009 | ClaudeMdPointerValid | gate (project) | ✓ |
| 010 | NoPushedCommits | exit (pre-push) | ✓ |
| 011 | OpenPrExists | exit | ✓ |
| 012 | NoUncommittedSrcChanges | exit | ✓ |
| 013 | MilestoneFilenameNoSuffix | gate | ✓ |
| 014 | NoMilestoneGaps | gate | ✓ |
| 015 | MilestoneConsistencyAudit | gate | ✓ |
| 016 | NoPrintlnInSrc | rule (+exit) | ✓ |
| 017 | TicketFilenameMatchesId | gate | ✓ |
| 018 | BugTicketMentionsTest | gate (bug) | ✓ |
| 019 | NoFormatWithBash | rule | ✓ |
| 020 | RawStringHashMismatch | exit | ✓ |
| 021 | NoBugsAtBetaPromotion | entry (beta gate) | ✓ |
| 022 | MilestoneHasGoal | gate | ✓ |
| 023 | MilestonePhaseValueValid | gate | ✓ |
| 024 | DoneTicketInDoneDir | gate | ✓ |
| 025 | NoAllowWithoutJustification | rule (+exit) | ✓ |
| 026 | TicketHasStatusField | gate | ✓ |
| 027 | CommitSubjectHasTicketId | rule (+exit) | ✓ |
| 028 | BranchNameFollowsScheme | rule | ✓ |
| 029 | NoUnresolvedDiscAtBeta | entry (beta/cycle gate) | ✓ |
| 030 | BugTicketInPatchMilestone | gate | ✓ |
| 031 | FeatTicketInFeatureMilestone | gate | ✓ |
| 032 | PatchMilestoneOnlyBugs | gate | ✓ |
| 033 | DoneDiscInDoneDir | gate | ✓ |
| 034 | BlockedTicketInProgress | gate | ✓ |
| 035 | DoneAuditInDoneDir | gate | ✓ |
| 036 | ReleaseBlockedWithoutAudit | entry (release gate) | ✓ |
| 037 | FeatMissingPhase | gate | ✓ |
| 039 | DoneMilestoneUntagged | gate (release) | ✓ |
| 040 | FeatHasOpenDesignQuestions | **entry (cycle feature-audit gate)** | ✓ |
| 041 | ConfigureAcVersionMatches | exit/gate (release) | ✓ |
| 042 | AutoMergeEnabled | gate (project) | ✓ |
| 043 | OneCommitPerFeature | rule (+exit) | ✓ |

## Summary by category

- **entry** (cycle/promotion preconditions): 021, 029, 036, **040** (the feature
  audit). → milestone-cycle entry gate (FEAT-086).
- **exit** (per-step acceptance): 001, 002, 010, 011, 012, 020 (+ the rule ones'
  exit role). → milestone-cycle exit gates (FEAT-087).
- **rule** (followed during work): 003, 004, 016, 019, 025, 027, 028, 043. →
  developer role rules (FEAT-088).
- **gate** (milestone/ticket hygiene at boundaries): the remainder. → milestone
  gate steps (later tranches; check.rs continues to provide these meanwhile).

`check.rs` stays as substrate this tranche; the wiring is additive.

## Port-or-retire disposition (FEAT-094)

Every check's final disposition toward retiring `check.rs`:

| Disposition | Checks | Where |
|-------------|--------|-------|
| **Ported to checks.rs** | 040, 029 (entry); 017/024/026/033/018 (ticket-hygiene); 013/022/005 (milestone-hygiene); 041/021 (release-gate); 003/004/016 (code-quality); **007/037 (ticket-targets, FEAT-094)** | `scripts/checks.rs` |
| **Covered by developer role rules** | 019, 025, 027, 028, 043 (+ 003/004/016 also rules) | `dwarfs/developer/role.toml` (FEAT-088) |
| **Runtime / git — covered by ci/push, not a static repo scan** | 001/002 (tests/clippy → ci); 010/011/012 (git state → push); 008/036 (release runtime); 039 (git tag); 042 (gh config) | ci / push workflows |
| **Resolved (DISC-022)** | 023 ported as `phase-valid`; 006 ports in 0.18.0 | conformed to vmodel |
| **Deferred to the 0.18.0 retirement tranche** | 009, 014, 015, 020, 030, 031, 032, 034, 035 (consistency/structural) | port alongside removing check.rs |

After DISC-022 resolves and the deferred consistency checks port in 0.18.0,
`check.rs` has no remaining responsibilities and the `check` verb reduces to
running `checks.rs`.
