# C++

> Language guidelines for C and C++ packages. Soft
> recommendations unless gated by a policy in `policies.md` or
> a hard rule in `.clang-tidy` / `.clang-format`. Compare with
> the per-package `CLAUDE.md` for package-specific conventions.

## Lint gate (binding)

`make lint` hard-fails on any clang-tidy diagnostic and any
clang-format diff over `src/**/*.{c,cpp,h,hpp}`. Concretely:

    clang-tidy -p build src/**/*.{c,cpp}
    clang-format --dry-run --Werror src/**/*.{c,cpp,h,hpp}

Both tools consume a `compile_commands.json` produced by the
configure step (cmake's `CMAKE_EXPORT_COMPILE_COMMANDS=ON`,
meson's built-in, or `bear` for raw make).

Waiver process:

- Repo-wide config in `.clang-tidy` (the `Checks:` list) and
  `.clang-format` (style + overrides). Each disabled check
  carries a one-line reason in a sibling comment.
- Inline waivers:
  `// NOLINT(<check>) // <reason>` for a single line,
  `// NOLINTNEXTLINE(<check>) // <reason>` for the following
  line, `// NOLINTBEGIN(<check>) ... // NOLINTEND` for blocks.
  The reason is **mandatory**.
- Format-only escapes: `// clang-format off` /
  `// clang-format on` around tables or ASCII art.

Do not introduce new diagnostics. If a code change would
create one, either fix the underlying issue or add an explicit
waiver with reason; the lint target rejects unreasoned
disables.

## Build / test invocation

These are the canonical mappings from the generic `make`
targets in `.repo/project/skills/testing.md` to the cmake
backend (meson and autotools are analogous):

    make check-unit  → ctest --test-dir build -L unit --output-on-failure
    make check-sit   → ctest --test-dir build -L sit  --output-on-failure
    make compile     → cmake --build build -j
    make lint        → clang-tidy + clang-format (see above)
    make install     → cmake --install build --prefix "$PREFIX"

The `build/` directory is produced by `cmake -B build -S .
-DCMAKE_EXPORT_COMPILE_COMMANDS=ON`. For autotools,
`./configure && make` substitutes; for meson, `meson setup
build && meson compile -C build`.

## Project layout

Canonical layout (cmake-flavoured; adapt names for other
backends):

    CMakeLists.txt       # top-level; add_subdirectory per component
    .clang-tidy
    .clang-format
    cmake/               # find-modules and toolchain files
    include/<pkg>/       # public headers (installed)
    src/                 # implementation + private headers
        <module>.cpp
        <module>.hpp     # private; never installed
    tests/
        unit/
            test_<module>.cpp
        sit/
    third_party/         # vendored deps; one subdir per dep
    build/               # gitignored; out-of-source build tree

Public headers live under `include/<pkg>/` so consumers
`#include <pkg/foo.hpp>`. Private headers stay next to the
`.cpp` they support.

## Idiom recommendations

- RAII for every resource — file handles, mutexes, sockets,
  allocations. If you write `new`, you should be writing
  `std::make_unique`.
- Smart pointers over raw owning pointers: `std::unique_ptr`
  by default, `std::shared_ptr` only when ownership is truly
  shared. Raw pointers are non-owning observers.
- `const`-correctness on parameters, member functions, and
  locals. `constexpr` where the compiler can evaluate it.
- `#include "..."` for project-local headers, `#include <...>`
  for system and third-party. Group and sort.
- Modern C++ baseline is **C++17** (structured bindings,
  `std::optional`, `std::variant`, `if constexpr`). C++20 if
  the toolchain target allows it.
- Prefer value semantics; move where copies are expensive.
  Rule of zero — let the compiler synthesise special members
  unless you genuinely need otherwise.
- `enum class` over plain `enum`; `nullptr` over `NULL` / `0`.
- No `using namespace std;` at file scope in headers.

## Testing

Pick one framework per project and stick with it — common
choices: **GoogleTest**, **Catch2**, **doctest**. File
convention:

    tests/unit/test_<module>.cpp
    tests/sit/test_<scenario>.cpp

Each test binary registers with ctest via
`add_test(NAME ... COMMAND ...)` and is labelled `unit` or
`sit` (`set_tests_properties(... PROPERTIES LABELS unit)`),
which is what `make check-unit` / `make check-sit` filter on.

Use the framework's fixture facility for shared setup; avoid
global state. Sanitisers (`-fsanitize=address,undefined`) on
the unit build are cheap insurance.

The cross-walk to `.repo/project/skills/testing.md`:
`tests/unit/` is the unit layer (links against the library
under test), `tests/sit/` is SIT (podman fixtures alongside
in `tests/sit/podman/`), `tests/pit/` is PIT.

## Reading

- *C++ Core Guidelines* —
  <https://isocpp.github.io/CppCoreGuidelines/>
- *Google C++ Style Guide* (lots of projects follow it
  closely) — <https://google.github.io/styleguide/cppguide.html>
- *cppreference* — <https://en.cppreference.com/>
- *clang-tidy checks* —
  <https://clang.llvm.org/extra/clang-tidy/checks/list.html>
- Per-package `CLAUDE.md`, `.clang-tidy`, `.clang-format` (the
  explicit known-debt list and idiom waivers, with reasons)
