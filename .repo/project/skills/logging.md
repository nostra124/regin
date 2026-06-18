---
name: logging
description: |
  Standard logging contract for scripts and binaries in
  this project. Four levels (debug / info / warn / error)
  plus terminal exits (fatal / die). Trigger when adding
  a new binary, when refactoring an existing one's
  logging surface, when CI failure logs are too sparse
  to diagnose, or when reviewing whether output goes to
  the right stream.
---

# `logging` skill

## 1. The four levels

| Level   | Stream | Gated by                    | Use for                                          |
|---------|--------|-----------------------------|--------------------------------------------------|
| `debug` | stderr | `$SELF_DEBUG`               | values, function-entry traces, branch decisions  |
| `info`  | stderr | suppressed by `$SELF_QUIET` | "starting X", "X complete", normal progress      |
| `warn`  | stderr | always                      | recoverable issue; the run continues             |
| `error` | stderr | always                      | failure of a sub-operation; caller decides next  |

Plus the two **terminal** helpers:

| Helper | Stream | Behaviour                                 |
|--------|--------|-------------------------------------------|
| `fatal "$msg" [code]` | stderr | print `<self>: fatal - <msg>`, exit `<code or 1>` |
| `die   "$msg" [code]` | stderr | print `<msg>` raw, exit `<code or 1>` (use in tightly-scoped places like the help dispatcher) |

## 2. Canonical implementation

The concrete helper implementations are
language-dependent and live in
`.repo/project/skills/language/<lang>.md`. Drop the
reference snippet from the language file into every new
script or binary verbatim — don't reinvent the shape.
The unit-test helpers and CI log-parsers assume the
prefix scheme described in §5.

## 3. Rules

1. **All non-stdout output is stderr.** `stdout` is for
   *the data the command produces* — version strings,
   task IDs, JSON. Help text, progress, warnings, and
   errors go to stderr. Pipelines must not get
   contaminated.
2. **`info` is gated by `$SELF_QUIET`, not by debug.**
   Quiet mode suppresses progress chatter; debug mode
   adds verbosity. They're independent axes.
3. **`debug` must include enough context to localise.**
   Print the function name (`debug "command:run: $WINDOW"`),
   the value (`debug "PID=$PID"`), and the branch
   decision (`debug "task $1 not running, skipping"`).
   A debug line without context is not a debug line.
4. **`warn` is for the run-continues case.** If you're
   about to fail the operation, use `error` + `fatal`
   or `die`. `warn` says "noticed something; carried on".
5. **`error` does not exit.** It reports and returns to
   the caller. The caller decides whether to `fatal`.
6. **`fatal` always exits non-zero.** Default code 1;
   pass an explicit code only when callers branch on it
   (rare).
7. **No ANSI codes in `error` / `warn` / `fatal`.**
   CI captures these as plain text into PR comments;
   colour escape codes render as garbage and make the
   diagnosis harder. `debug` and `info` may use ANSI
   when locally useful, but prefer plain.

## 4. Worked examples

1. **Normal progress, gated by quiet.**

       info "starting build of $PACKAGE"

2. **Recoverable hiccup.**

       warn "lockfile older than manifest; using manifest"

3. **Sub-operation failed; caller will decide.**

       error "could not extract $TARBALL"
       return 1

4. **Operation failed; abort cleanly.**

       fatal "package $PACKAGE locally not found"

5. **Help-dispatcher unknown verb.**

       die "'$1' is not an $SELF command"

## 5. Test contract

Unit-test suites assume the level prefixes above.
Greppable patterns used in regression tests look for the
`warn -` / `error -` / `fatal -` substrings on stderr.

If you change the prefix style, the existing test
greps break — bump the test contract in the same PR.

## 6. CI-readability contract

When CI fails, the workflow tails the unit-test output
into a PR comment (`.repo/project/skills/testing.md` §
3). For the agent subscribed to PR activity to read it
correctly:

- Every `error` / `warn` / `fatal` line must be
  self-contained (no "see previous line").
- Every error line must name the *resource* that
  failed and the *operation* that failed on it.
- A grep for `error|fatal|warn` against the log
  should reveal the root cause without scrolling.

## 7. Where to read more

- CLAUDE.md § 9 — logging contract summary
- `.repo/project/skills/language/<lang>.md` — concrete
  helper implementations for the project's language
- Reference implementations: see the binaries shipped
  with this project (look under `bin/` and `libexec/`).
