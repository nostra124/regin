# RULE-004 — PIT tests cover package installation methods

scope: full
severity: block

## Rule

PITs must test every supported installation method. At minimum:

1. **Debian package** (`.deb`) — install via `apt` or `dpkg -i`
2. **Alpine package** (`.apk`) — install via `apk add`
3. **Source tarball** — `./configure && make && make install` from the
   release `.tar.gz`

Each installation method must be tested in its own podman container using
the appropriate base image. The test must verify the installed binary works
end-to-end, not just that the package installs without errors.

## Pass criteria

- `tests/pit/` contains at least three test files: `deb-install.bats`,
  `apk-install.bats`, `source-install.bats` (exact names may vary).
- Each test runs in a dedicated podman container from a clean base image
  (e.g. `debian:stable-slim`, `alpine:latest`, `ubuntu:latest`).
- Each test exercises at least one real functional scenario after install.

## Fail criteria

- Only one installation method tested.
- Tests check only that the package installs, not that it functions.
- No dedicated container per installation method.
- `docker` used instead of `podman`.

## Audit instruction

List the PIT test files. For each, identify: what installation method,
what base image, what functional scenario. Report which of the three
required methods (deb, apk, source) are missing. Flag any test that only
checks install exit code without testing functionality.
