---
id: FEAT-014
type: feature
status: open
milestone: 0.3.0
disc: DISC-007
---
# FEAT-014 — Skill-package structure (regin-base + role/area packages)

regin's skills ship as packages dvalin deploys (dvalin FEAT-128). Define the
package layout (`regin-<base|role|area>-skills`: a manifest + skill files) and a
loader/installer: `regin skill install <pkg-dir>` copies a package's skills into
the user skills store; `regin skill packages` lists installed packages.
- package manifest parse + skill enumeration (unit-tested).
- install into the skills dir (idempotent).

Acceptance: a package manifest parses; install lands its skills in the store and
is idempotent. Unit-tested over temp dirs.
