# RULE-002 — SIT tests use bats for shell/CLI projects; language-native for others

scope: full
severity: block

## Rule

System Integration Tests (SIT) must use the framework appropriate to the
project's primary language:

| Project language | SIT framework | Runner |
|---|---|---|
| Shell / bash | bats (Bash Automated Testing System) | `make check-sit` → `bats tests/sit/` |
| Rust | bats (tests the installed binary as a black box) | `make sit` → `bats tests/sit/` |
| Python | pytest | `make check-sit` → `pytest tests/sit -q` |
| Go | `go test -tags=sit` | `make check-sit` → `go test -tags=sit ./...` |
| JavaScript/Node | Jest or Vitest | `make check-sit` → `npx jest --testPathPattern=sit` |
| C/C++ | bats (tests the installed binary) | `make check-sit` → `bats tests/sit/` |

The language in use is declared in `.repo/project/profile.md`.

SIT tests exercise the **installed or locally-built binary/library as a black
box** — they are not unit tests and must not import internal modules.

SIT tests run **locally without containers**. If a test requires a container it
is a PIT, not a SIT (see RULE-003).

## Pass criteria

- The SIT framework matches the project's primary language (table above).
- SIT tests live under `tests/sit/`.
- `make check-sit` (or equivalent) runs the SIT suite without containers.
- Shell/Rust/C projects: `bats` is listed as a build dependency in
  `configure.ac` (`AC_CHECK_PROG([BATS], [bats], [bats])`).
- SIT tests make no container invocations (`podman`, `docker`).

## Fail criteria

- SIT tests use a framework that does not match the project's language.
- SIT tests spin up containers (those belong in `tests/pit/`).
- Shell/Rust/C project: no `bats` dependency check in `configure.ac`.
- No `make check-sit` target or equivalent.

## Audit instruction

1. Read `.repo/project/profile.md` to determine the project's primary language.
2. Locate the SIT test directory (`tests/sit/`). Confirm the test files use the
   correct framework for that language (table above).
3. Confirm `make check-sit` exists and runs without containers.
4. For shell/Rust/C projects: confirm `configure.ac` checks for `bats`.
5. Scan test files for `podman` or `docker` invocations — any found are PIT
   tests misplaced in the SIT directory. List them.
6. Report PASS or FAIL with findings.
