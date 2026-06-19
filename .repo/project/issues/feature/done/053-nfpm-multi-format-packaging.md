---
id: FEAT-053
type: feature
priority: high
complexity: M
estimate_tokens: 50k-90k
estimate_time: 90-150min
phase: open
status: open
milestone: 0.5.0
spawned_from: DISC-014
depends_on: FEAT-019
---

# FEAT-053 — nfpm multi-format packaging (deb + rpm + apk)

## Description
**As** a packager
**I want** one recipe that builds deb, rpm, and apk from source
**So that** regin installs cleanly on Debian/Ubuntu, RHEL/Fedora/SUSE, and Alpine with
no per-format drift.

**Supersedes / reworks FEAT-020** (deb-only) into a single nfpm recipe.

## Implementation
- A single **`nfpm`** config generates `.deb` + `.rpm` + `.apk` from source; version
  stamped from `Cargo.toml` (single source of truth).
- Ships the generated man pages (FEAT-019) and the `regin-operator-skills` package
  (FEAT-046); installs the per-user systemd integration as today.
- No checked-in staging artifacts (RULE-011); the recipe is committed and repeatable.
- **profile §7 update:** deb-only first-class → **deb/rpm/apk first-class**; remove the
  apk/rpm DISC gate.

## Acceptance Criteria
1. One `nfpm` invocation produces deb, rpm, and apk, each carrying the `Cargo.toml`
   version and the generated man pages.
2. Each package installs and runs on its target distro family (verified by FEAT-054).
3. No build/staging artifacts remain in git; `profile.md` §7 lists deb/rpm/apk as
   first-class.
