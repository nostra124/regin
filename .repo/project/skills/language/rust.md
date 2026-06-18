# Rust

> Language guidelines for rust crates and workspaces. Soft
> recommendations unless gated by a policy in `policies.md` or a
> hard rule in `clippy.toml` / `rustfmt.toml`. Compare with the
> per-package `CLAUDE.md` for package-specific conventions.

## Lint gate (binding)

`make lint` hard-fails on any clippy warning over the workspace.
Concretely:

    cargo clippy --workspace --all-targets -- -D warnings

Waiver process:

- Repo-wide allows in `clippy.toml` (each entry carries a
  one-line reason).
- Inline waivers on a specific item:
  `#[allow(clippy::<lint>)] // <reason>`. The reason is
  **mandatory**.

Formatting is enforced via `cargo fmt --all -- --check` in the
same gate. `rustfmt.toml` lives at the workspace root; do not
override locally without a comment.

Do not introduce new clippy warnings. If a code change would
create one, either fix the underlying issue or add an explicit
allow with reason; the lint target rejects unreasoned allows.

## Build / test invocation

These are the canonical mappings from the generic `make`
targets in `.repo/project/skills/testing.md` to cargo:

    make check-unit  → cargo test --workspace --lib --bins
    make check-sit   → cargo test --workspace --tests
    make compile     → cargo build --workspace
    make lint        → cargo clippy --workspace --all-targets -- -D warnings
    make install     → cargo install --path . --locked

`make check-pit` maps to a project-specific `cargo test
--workspace --features pit` (or a separate harness binary)
when the package opts into PIT.

## Project layout

Canonical Cargo workspace layout:

    Cargo.toml          # [workspace] root; pins members + shared deps
    Cargo.lock          # checked in for binaries; gitignored for libs
    rust-toolchain.toml # pin channel/version for reproducibility
    crates/<name>/
        Cargo.toml
        src/
            lib.rs      # or main.rs for binaries
            <module>.rs
        tests/          # integration tests (one binary per file)
        benches/        # criterion benches (optional)
    target/             # gitignored; do not check in build artefacts

Single-crate projects collapse `crates/<name>/` into the root.
The `target/` directory is shared workspace-wide.

## Idiom recommendations

- Prefer `Result<T, E>` over panics; reserve `panic!` for
  truly-unrecoverable invariants. Use `?` for propagation.
- Define error types per crate (`thiserror` for libraries,
  `anyhow` for binaries is common). Don't leak `Box<dyn Error>`
  through public APIs.
- Module structure: declare with `mod <name>;` in the parent;
  prefer `<name>.rs` siblings over `<name>/mod.rs` (2018+ style).
- Borrow over clone. `&str` and `&[T]` in function signatures
  unless ownership is required.
- `#[must_use]` on functions whose return value carries
  significance (builders, status enums).
- Iterators over index loops; `.collect::<Result<Vec<_>, _>>()`
  is the standard pattern for fallible mapping.
- `clippy::pedantic` is opt-in per crate — turn it on if the
  team agrees, off by default.
- Public APIs get doc comments (`///`) and at least one
  `# Examples` block; `cargo test --doc` runs them.

## Testing

Two layers, both invoked by `cargo test`:

- **Unit tests** — inline `#[cfg(test)] mod tests { ... }` at
  the bottom of each module. Access to private items.
- **Integration tests** — `tests/<name>.rs` files at the crate
  root. Each file compiles to a separate binary; only the
  public API is in scope.

Fixtures and shared helpers go under `tests/common/mod.rs`
(the `mod.rs` form prevents cargo from treating it as a test
binary). `#[test]` attribute on each test fn; `#[should_panic]`
+ `expected = "..."` for expected-panic cases.

The cross-walk to `.repo/project/skills/testing.md`: cargo's
`--lib --bins` is the unit layer, `--tests` is SIT. PIT, when
present, is gated behind a `pit` feature.

## Reading

- *The Rust Programming Language* (the Book) —
  <https://doc.rust-lang.org/book/>
- *Rust API Guidelines* —
  <https://rust-lang.github.io/api-guidelines/>
- *Clippy lint index* —
  <https://rust-lang.github.io/rust-clippy/master/>
- *The Rustonomicon* (for unsafe code) —
  <https://doc.rust-lang.org/nomicon/>
- Per-package `CLAUDE.md` and `clippy.toml` (the explicit
  known-debt list and idiom waivers, with reasons)
