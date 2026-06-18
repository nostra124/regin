# RULE-001 — Unit tests implemented in native project language

scope: full
severity: block

## Rule

Unit tests (`make check` target) must be written in the same language as the
production code. No shell scripts, no Python wrappers around a C project, no
bats files masquerading as unit tests.

- C/C++ project → CUnit, Check, Google Test, or equivalent
- Python project → pytest or unittest
- Rust project → built-in `#[test]` / `cargo test`
- Java project → JUnit

## Pass criteria

- The `make check` target runs the native test framework directly.
- Test files live alongside or adjacent to the source they test.
- No shell scripts in the unit test target.

## Fail criteria

- `make check` invokes a shell script that runs tests rather than the
  native test binary.
- Tests written in a different language than the source (e.g. Python tests
  for a C project in the `make check` target).

## Audit instruction

Review the `Makefile.am` (or equivalent) `check` target. List any test
invocations that are not in the native project language. For each: is this
a unit test or a SIT/PIT test that belongs in a different target?
