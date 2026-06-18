# RULE-003 — Process interaction tests use podman (never docker)

scope: full
severity: block

## Rule

Process Interaction Tests (PIT) always run inside podman containers.
Docker is never used — podman is the mandatory container runtime for all
projects managed by dvalin. PITs test the full process lifecycle: install
from package, configure, start, interact, stop, uninstall.

## Pass criteria

- All PIT tests are `.bats` files under `tests/pit/` (or equivalent).
- Container invocations use `podman`, never `docker`.
- The `make pit` target invokes `bats` against the `tests/pit/` directory.
- `podman` is listed as a dependency in `configure.ac`.

## Fail criteria

- Any `docker` invocation in PIT test files or Makefile targets.
- PIT tests that run outside a container (no isolation).
- `docker-compose`, `docker run`, `docker build` anywhere in the test suite.

## Audit instruction

Grep `tests/pit/` and all Makefile targets for `docker`. Any match is a
violation. Confirm `podman` is the only container runtime referenced.
Confirm `configure.ac` checks for `podman` (`AC_CHECK_PROG`).
