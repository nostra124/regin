# Shell (bash)

> Language guidelines for bash scripts. Soft recommendations
> unless gated by a policy in `policies.md` or a hard rule in
> `.shellcheckrc`. Compare with the per-package `CLAUDE.md`
> for package-specific conventions.

## Lint gate (binding)

`make lint` hard-fails on any shellcheck warning over `bin/*` +
`libexec/**/*`. The waiver process is documented in `tests/README.md`
of the parent collection (FEAT-209). In short:

- Repo-wide waivers in `.shellcheckrc` (each entry carries a
  one-line reason).
- Inline waivers on a specific line: `# shellcheck disable=SCxxxx
  # <reason>`. The reason is **mandatory**.

Do not introduce new shellcheck warnings. If a code change would
create one, either fix the underlying issue or add an explicit
waiver with reason; the lint target rejects unreasoned disables.

## Shebang

```bash
#!/bin/bash
```

Files at `bin/<pkg>` and `libexec/<pkg>/<verb>` are bash; do not
target POSIX `sh` unless a specific subset is needed (e.g. `.cpk/`
hooks may be `/bin/sh` for the rootfs that doesn't ship bash).

## Strict mode (project-by-project)

The collection's foundation scripts deliberately do **not** use
`set -euo pipefail` because the dispatcher pattern relies on
specific exit-code propagation. New code may use strict mode as
long as it doesn't break the `command:<verb>` dispatcher
contract.

If you add strict mode, document the choice in the script header
and ensure the test suite exercises every error path.

## Dispatcher pattern

Every package's `bin/<pkg>` follows the same shape (canonical
example: `bin/account`):

```bash
[ -n "$SELF_DEBUG" ] && set -vx

# Per FEAT-194: VERSION read at runtime from .rpk/version (dev
# tree) or $PREFIX/share/<pkg>/version (installed). Hardcoded
# fallback only if both files are missing.
__d="$(cd "$(dirname "$0")" && pwd)"; __s="$(basename "$0")"
VERSION=$(cat "$__d/../.rpk/version" 2>/dev/null || cat "$__d/../share/$__s/version" 2>/dev/null || echo '0.1.0')
unset __d __s
SELF=$(basename "$0")

# helpers
fatal()  { echo "$SELF: fatal - $1" >&2; exit "${2:-1}"; }
debug()  { [ -n "$SELF_DEBUG" ] && echo "$SELF: debug - $*" >&2; }
has() { type -t "$1:$2" | grep -q function; }

# getopts
while getopts "dq" flag; do ...; done; shift $((OPTIND - 1))

# command:<verb> functions
command:help()    { ...; }
command:version() { echo "$VERSION"; }
command:foo()     { ...; }

# dispatch tail
[[ $# == 0 ]] && { command:help; exit 0; }
if has command "$1"; then
    command:"$1" "${@:2}"
    exit $?
else
    fatal "unknown command: $1"
fi
```

Idiom is intentional and replicated per-package. **Do not extract
a shared `lib/common.sh`** — duplication is the policy
(`CLAUDE.md.foundation` § 4–5).

## Sub-services via libexec

Long sub-commands move to `libexec/<pkg>/<verb>`:

```bash
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
LIBEXEC="$SELF_DIR/../libexec/$SELF"

command:foo() { exec "$LIBEXEC/foo" "$@"; }
```

Resolved relative to the script — works in dev tree and after
`stow` install.

## Quoting

Default to **quoting variables** (`"$x"`, not `$x`). Exceptions:

- Word-splitting is intentional (passing through to a sub-command
  that re-splits): `cmd "$@"`, then unquoted `$@` rarely.
- Glob expansion is intentional: `for f in *.txt; do`.

The codebase's `.shellcheckrc` waives SC2046 / SC2068 / SC2086
because the deliberate-word-splitting pattern recurs; new
unquoted expansions still need a defensible reason.

## `[ ... ]` vs `[[ ... ]]`

- `[[ ... ]]` for string comparisons, regex, and pattern matches.
  No word-splitting on unquoted right-hand-side; safer.
- `[ ... ]` only when targeting POSIX `sh`.

For a bash script, prefer `[[ ... ]]` unless there's a reason.

## Exit codes

- `0` for success.
- `1` for general failure; pass through `${?:-1}` for child
  failures.
- `2..127` for specific error classes (document them).
- Avoid `return -1` — it wraps to 255 and is parsed inconsistently.
  Use `return 1`. (The codebase's existing widespread `return -1`
  is waived in `.shellcheckrc`; new code shouldn't add to the
  debt.)

## Testing

Every package's `tests/unit/<pkg>.bats` follows:

```bash
setup() {
    BATS_TMPDIR=${BATS_TMPDIR:-$(mktemp -d)}
    HOME="$(mktemp -d "$BATS_TMPDIR/home.XXXXXX")"
    unset XDG_CACHE_HOME XDG_CONFIG_HOME XDG_DATA_HOME ...
    export HOME
    export SELF_QUIET=1
    export <PKG>_BIN="$BATS_TEST_DIRNAME/../../bin/<pkg>"
}

teardown() { rm -rf "$HOME"; }

@test "help mentions <verb>" { ... }
```

The `$BATS_TEST_DIRNAME/../../bin/<pkg>` path is **symmetric** —
works from `tests/unit/<pkg>.bats` AND from `<pkg>/tests/unit/<pkg>.bats`.

For SIT, fixture lives at `tests/sit/podman/Dockerfile.<pkg>` +
suite under `tests/sit/suites/*.bats`. The container brings up
the package's runtime dependencies (real binaries, regtest
nodes, etc.).

## Reading

- `man bash` (the canonical reference)
- shellcheck wiki: <https://www.shellcheck.net/wiki/>
- The codebase's `.shellcheckrc` (the explicit known-debt list
  and idiom waivers, with reasons)
- Per-package `CLAUDE.md` (package-specific conventions)
- The `<pkg>/share/doc/<pkg>/standards/` directory (per-package
  vendored references)
