# Go

> Language guidelines for go modules. Soft recommendations
> unless gated by a policy in `policies.md` or a hard rule in
> `.golangci.yml`. Compare with the per-package `CLAUDE.md`
> for package-specific conventions.

## Lint gate (binding)

`make lint` hard-fails on any golangci-lint issue and any
`gofmt -l` diff over the module. Concretely:

    golangci-lint run ./...
    test -z "$(gofmt -l .)"

`gofmt` (or `goimports`, the superset) is non-negotiable —
formatting is part of the language, not a style choice.

Waiver process:

- Repo-wide config in `.golangci.yml` (the `linters:` and
  `issues:` blocks). Each disabled linter carries a one-line
  reason comment.
- Inline waivers on a specific line:
  `//nolint:<rule> // <reason>`. The reason is **mandatory**.
  Note: no space between `//` and `nolint` — golangci-lint
  parses the form strictly.

Do not introduce new linter findings. If a code change would
create one, either fix the underlying issue or add an explicit
waiver with reason; the lint target rejects unreasoned
disables.

## Build / test invocation

These are the canonical mappings from the generic `make`
targets in `.repo/project/skills/testing.md` to go tooling:

    make check-unit  → go test -short ./...
    make check-sit   → go test -tags=sit ./...
    make compile     → go build ./...
    make lint        → golangci-lint run ./... && test -z "$(gofmt -l .)"
    make install     → go install ./cmd/...

`make check-pit` maps to `go test -tags=pit ./...` when the
package opts into PIT. Race detector on the unit layer
(`go test -race -short ./...`) is cheap and recommended.

## Project layout

Canonical module layout (matches the de-facto community
standard; not an official spec):

    go.mod               # module path + Go version + deps
    go.sum               # checked in
    cmd/<bin>/main.go    # one subdir per binary
    internal/<pkg>/      # private to this module; cannot be imported externally
        <file>.go
        <file>_test.go   # tests live next to the code
    pkg/<pkg>/           # public API; importable by external modules
    testdata/            # fixtures (the `testdata` name is special — ignored by go tool)
    tools/               # build-only tooling, pinned via tools.go

Package paths are lower-case, no underscores, short. The
directory name **is** the package name (with rare exceptions).

## Idiom recommendations

- Explicit error returns over panics. The standard idiom is
  `if err != nil { return ..., fmt.Errorf("doing X: %w", err) }`
  — wrap with `%w` so callers can `errors.Is` / `errors.As`.
- `context.Context` as the first argument to any function that
  does I/O, RPC, or is otherwise cancellable. Never store a
  context in a struct.
- Accept interfaces, return concrete types. Define interfaces
  in the consuming package, not the producing one.
- `gofmt` defaults are sacrosanct — tabs for indent, no
  alignment tricks; let the tool decide.
- Table-driven tests with subtests
  (`t.Run(tc.name, func(t *testing.T) { ... })`); makes
  failure output precise.
- Receiver names are short and consistent across methods on
  the same type (`func (s *Server) Foo()` everywhere, not
  `(self *Server)`).
- Exported identifiers get doc comments starting with the
  identifier name (`// Foo does X.`). Linted by `revive` /
  `golint`.
- Channels for ownership transfer between goroutines; mutexes
  for shared-state protection. Don't mix the two patterns on
  the same field.

## Testing

`go test` is the framework. Conventions:

- Files: `<file>_test.go` next to the code; `package foo`
  for white-box tests, `package foo_test` for black-box
  (public-API-only) tests.
- Functions: `func TestXxx(t *testing.T)`. Subtests via
  `t.Run`.
- Fixtures: anything in `testdata/` is ignored by build tools
  and free to use. `t.TempDir()` for scratch dirs.
- Build-tag gating: `//go:build sit` (line 1 of the file, blank
  line after) for tests that should only run under
  `go test -tags=sit`. Keeps SIT and PIT off the default path.
- Benchmarks: `func BenchmarkXxx(b *testing.B)`; run via
  `go test -bench=. -benchmem`.

The cross-walk to `.repo/project/skills/testing.md`:
`go test -short ./...` is the unit layer, `-tags=sit` is SIT
(podman fixtures alongside in `tests/sit/podman/`),
`-tags=pit` is PIT.

## Reading

- *Effective Go* — <https://go.dev/doc/effective_go>
- *Go Code Review Comments* —
  <https://github.com/golang/go/wiki/CodeReviewComments>
- *Standard library docs* — <https://pkg.go.dev/std>
- *golangci-lint linters* —
  <https://golangci-lint.run/usage/linters/>
- Per-package `CLAUDE.md` and `.golangci.yml` (the explicit
  known-debt list and idiom waivers, with reasons)
