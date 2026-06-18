# RULE-006 — No stub implementations left behind

scope: full
severity: block

## Rule

A shipped feature must not leave stub implementations in the codebase.
Stubs signal that a feature was declared done before it was actually complete.
Every stub must be tracked by a BUG or FEAT ticket so no silent debt accumulates.

A stub is any of:
- `todo!()`, `unimplemented!()`, `panic!("not implemented")` in production code
- Functions whose body is a single `Ok(())` or `""` with no logic and no test
  exercising the real behaviour
- Placeholder comments: `// TODO`, `// STUB`, `// FIXME`, `// NYI`,
  `// placeholder`, `// not implemented`
- Trait implementations that delegate every method to `unimplemented!()`

Test helpers and intentional no-ops are exempt when annotated with
`// intentional no-op` or `#[cfg(test)]`.

## Why stubs persist

Common causes observed in this codebase:

1. **Phase boundary pressure** — a FEAT was moved to `done/` at the end of the
   design or implement phase without verifying the integrate phase requirements.
   Fix: enforce that `done/` moves happen only after the integrate phase passes.

2. **Missing test coverage** — stubs survive because no test exercises the stub
   path. The failing test would have caught it.
   Fix: RULE-001 (unit tests in native language) must cover every public function.

3. **Incremental scaffolding without tickets** — a developer adds a function
   shell to unblock compilation, intending to return. Without a ticket the
   intention is invisible to the conductor.
   Fix: any scaffold added to pass compilation but not yet implemented must be
   accompanied by a BUG ticket filed in the same commit.

## Pass criteria

- `grep -r 'todo!\|unimplemented!\|// TODO\|// STUB\|// FIXME\|// NYI\|// placeholder\|// not implemented' src/`
  returns no matches in non-test code.
- Every function reachable from `main` has at least one test that exercises a
  non-trivial code path (i.e., not just `Ok(())`).
- No open BUG ticket has a title containing "stub" or "placeholder" that is
  not assigned to the current milestone.

## Fail criteria

- Any production source file contains `todo!()` or `unimplemented!()`.
- Any production source file contains a comment matching the patterns above.
- A function's entire body is `Ok(())` or `String::new()` with no covering test.
- An open BUG ticket exists for a stub but is not in the active milestone's
  ticket list (i.e., it is silently deferred).

## Audit instruction

1. Run: `grep -rn 'todo!\|unimplemented!\|// TODO\|// STUB\|// FIXME\|// NYI\|// placeholder\|// not implemented' src/`
   Exclude `#[cfg(test)]` blocks. List every hit with file and line number.

2. For each stub found:
   a. Determine which FEAT ticket introduced it (git log -S).
   b. Check whether a BUG ticket already exists for it.
   c. If no BUG ticket exists, file one now: `BUG-NNN-stub-<function-name>.md`
      with `blocked_by:` pointing to the original FEAT if relevant.

3. Analyse root cause using the three categories above (phase boundary pressure,
   missing test coverage, incremental scaffolding). State which category applies
   to each stub and what process change would have prevented it.

4. For each stub: propose the concrete fix (implement it, or raise it as a
   scoped BUG ticket with acceptance criteria and a failing test).

5. Report PASS if the grep returns zero hits. Report FAIL with the full list
   otherwise and a count of open BUG tickets filed as a result of this audit.
