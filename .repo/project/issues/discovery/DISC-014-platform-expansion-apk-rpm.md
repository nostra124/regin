---
id: DISC-014
type: discovery
priority: medium
status: open
complexity: M
spawned_features: ~
---

# DISC-014 — Platform-list expansion to `.apk` + `.rpm`

## Operating-plane context

Operator plane / delivery. **profile §7 currently makes `.deb` the only first-class
platform; apk/rpm expansion is explicitly DISC-gated here.** Broadening the package
list widens the standalone-operator (Mode-B) install base.

## Describe

regin is a standalone Linux operator, but only ships a Debian/Ubuntu `.deb`. FEAT-020
makes that deb **regenerable from source**. Two large families are unserved:

- **`.rpm`** — RHEL / Fedora / openSUSE: the other half of the server world.
- **`.apk`** — Alpine: ubiquitous in **containers**, which is exactly where regin's
  container-portable identity (DISC-017) is meant to live.

Each added format costs packaging config, an install PIT (RULE-003/004, podman), and
ongoing maintenance. This DISC decides **which** formats, **how** they're built, and
**when**.

## Variants considered

| Point | Options | Leaning |
|---|---|---|
| Platforms | deb-only (status quo) · +rpm · +apk · **+both (rpm & apk)** | both — apk especially (container fit) |
| Build approach | native per-format (`dpkg-deb` / `rpmbuild` / `abuild`) · **single cross-format tool (`nfpm`: one config → deb/rpm/apk)** · `fpm` | `nfpm` — one source of truth, regenerable |
| Timing | **in 0.5.0** (all-platforms is an alpha delivery prerequisite) · defer to a later milestone | in 0.5.0, after FEAT-020 |

## Decision matrix

| Criterion | Weight | deb-only | +both via native tools | +both via nfpm (leaning) |
|---|---|---|---|---|
| Reaches RHEL/Fedora + Alpine/container users | high | ✗ | ✓ | ✓ |
| Single regenerable source of truth (no per-format drift) | high | ~ | ✗ | ✓ |
| Build/maintenance cost | med | ✓ | ✗ | ~ |
| Testable per RULE-004 (install PIT per format) | med | ✓ | ✓ | ✓ |

## Open questions (resolving with user)

1. **Platforms** — add both `.rpm` and `.apk`, or only one (which)?
2. **Build approach** — adopt `nfpm` (one config → deb + rpm + apk, replacing the deb
   recipe too), or build each format with its native toolchain?
3. **Timing** — land in 0.5.0 alongside FEAT-020 (alpha needs all-platforms), or defer
   to a later milestone?

(Install PIT per format via podman is assumed required by RULE-004, not an open
question.)

## Decision (resolved with user — guided Q&A 2026-06-19)

**Q1 — Platforms: both `.rpm` and `.apk`.** Add RHEL/Fedora/openSUSE (rpm) and Alpine
(apk) — apk especially, as regin's container-portable identity (DISC-017) lives on
Alpine. deb/rpm/apk all become first-class.

**Q2 — Build: `nfpm`, single config.** One `nfpm` config generates deb + rpm + apk,
**replacing** the standalone deb recipe (FEAT-020) too — a single regenerable source of
truth, no per-format drift.

**Q3 — Timing: in 0.5.0.** Lands alongside FEAT-020; "native packages, all platforms"
is an alpha delivery prerequisite and nfpm makes the marginal cost small.

**Consequence — profile §7 updated.** deb-only first-class becomes deb/rpm/apk
first-class; this DISC removes the apk/rpm gate. FEAT-020 is reworked to the nfpm recipe
rather than a deb-only one.

## Spawned features

- **nfpm-based multi-format packaging** — replace the FEAT-020 deb recipe with a single
  `nfpm` config producing deb + rpm + apk from source; version stamped from
  `Cargo.toml`; ships generated man pages (FEAT-019) + the operator skill package
  (DISC-012). Supersedes/extends **FEAT-020**. Milestone 0.5.0.
- **Per-format install PIT** — podman-based install tests for Debian/Ubuntu, Fedora/
  RHEL, and Alpine (RULE-003/004). Milestone 0.5.0.
- **profile §7 update** — make deb/rpm/apk first-class; drop the apk/rpm DISC gate.
  Milestone 0.5.0.
