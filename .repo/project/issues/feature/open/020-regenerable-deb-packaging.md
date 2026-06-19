---
id: FEAT-020
type: feature
priority: high
complexity: M
estimate_tokens: 40k-90k
estimate_time: 60-120min
phase: open
status: open
milestone: 0.5.0
---

# FEAT-020 — Regenerable `.deb` packaging (drop the checked-in staging tree)

> **Superseded by FEAT-053** (nfpm multi-format packaging, DISC-014). The deb recipe
> is folded into a single `nfpm` config that also produces rpm + apk; the "drop the
> checked-in staging tree / version from `Cargo.toml`" requirements carry over to
> FEAT-053. Kept here for history; do not implement separately.

## Description
**As a** maintainer
**I want** the Debian package built from source by a committed, repeatable recipe
**So that** packaging is reproducible, versioned correctly, and free of in-git artifacts

Today packaging is a **hand-built staging tree** committed at
`pkg/regin_0.2.0_amd64/` (DEBIAN/control + postinst/prerm + a copy of the
binaries' payload, README, skills). It is pinned at `0.2.0`, is not regenerable,
duplicates source (README, skills), and is a build/staging artifact in git
(RULE-011). Separately, `Cargo.toml` is still `version = "0.1.0"` while three
milestones have shipped — the package version and the workspace version disagree.

This ticket replaces the staging tree with a repeatable recipe for the **`.deb`**
(the only first-class platform in profile §7). Expansion to `.apk`/`.rpm` is a
separate platform-list decision (DISC-012) and is out of scope here.

## Implementation
- Adopt a regenerable recipe (e.g. `cargo-deb` with `[package.metadata.deb]`, or
  an `nfpm.yaml`) that builds `regin` + `regind`, installs the systemd user unit,
  the generated man pages (FEAT-019), and the system skills under
  `usr/share/regin/skills/`, with `postinst`/`prerm` preserved.
- Stamp the package version from `Cargo.toml`; **reconcile `Cargo.toml` to the
  current milestone version** as part of this ticket (single source of truth,
  profile §8).
- Remove the committed `pkg/regin_*_amd64/` staging tree; ensure build output is
  gitignored (RULE-011).
- PIT-test (per RULE-004): build the deb, install it in a clean container, verify
  binaries, unit, man pages, and skills land correctly (`dpkg --info` / file checks).

## Acceptance Criteria
1. A single committed command produces `regin_<ver>_amd64.deb` from a clean tree.
2. The produced version equals `Cargo.toml`'s version; `Cargo.toml` no longer
   disagrees with the shipped milestone.
3. No `pkg/regin_*_amd64/` staging tree (or other build artifact) remains in git.
4. Installing the deb in a clean container yields working `regin`/`regind`, the
   systemd user unit, man pages, and the system skill catalog (PIT-tested).
