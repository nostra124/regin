# JavaScript / TypeScript

> Language guidelines for node and browser packages. Soft
> recommendations unless gated by a policy in `policies.md` or
> a hard rule in `eslint.config.js` / `tsconfig.json`. Compare
> with the per-package `CLAUDE.md` for package-specific
> conventions.

## Lint gate (binding)

`make lint` hard-fails on any eslint diagnostic and any
`prettier --check` diff over the package. Concretely:

    eslint .
    prettier --check .

For TypeScript projects, the compiler joins the gate:

    tsc --noEmit

Waiver process:

- Repo-wide config in `eslint.config.js` (flat config; the
  modern form) or `.eslintrc.json` (legacy). Each disabled
  rule in the `rules:` block carries a one-line reason
  comment.
- Inline waivers on a specific line:
  `// eslint-disable-next-line <rule> -- <reason>`. The `--
  <reason>` clause is **mandatory** (enforced by
  `eslint-comments/require-description`).
- Type-check escapes: `// @ts-expect-error <reason>` (prefer)
  or `// @ts-ignore <reason>` (only when the error is
  conditional). Reason is **mandatory**.
- Format-only escapes: `// prettier-ignore` immediately above
  the line you want preserved.

Do not introduce new diagnostics. If a code change would
create one, either fix the underlying issue or add an explicit
waiver with reason; the lint target rejects unreasoned
disables.

## Build / test invocation

These are the canonical mappings from the generic `make`
targets in `.repo/project/skills/testing.md` to npm scripts
(yarn / pnpm map analogously). The `package.json` `scripts`
block bridges them:

    make check-unit  → npm run test:unit       # vitest run / jest --selectProjects unit
    make check-sit   → npm run test:sit        # vitest run --config sit / playwright test
    make compile     → npm run build           # tsc / vite build / esbuild
    make lint        → npm run lint            # eslint . && prettier --check .
    make install     → npm pack && npm install -g ./*.tgz   # for CLI packages

`make check-pit` maps to `npm run test:pit` when the package
opts into PIT.

## Project layout

Canonical layout for a TypeScript library or CLI:

    package.json         # name, type: module, scripts, deps
    package-lock.json    # checked in for apps and CLIs; libs may omit
    tsconfig.json        # strict: true at minimum
    eslint.config.js     # flat config
    .prettierrc          # or `prettier` key in package.json
    src/
        index.ts
        <module>.ts
    tests/
        unit/
            <module>.test.ts
        sit/
            <scenario>.test.ts
    dist/                # gitignored; tsc / bundler output
    node_modules/        # gitignored

`"type": "module"` in `package.json` opts into ESM —
preferred for new code. CommonJS interop via the `.cjs`
extension when an upstream dep demands it.

## Idiom recommendations

- ES modules (`import` / `export`) over CommonJS
  (`require` / `module.exports`) wherever the runtime allows.
- `'use strict';` is implicit in ES modules; in CommonJS files
  add it at the top.
- `const` by default; `let` only when reassignment is needed;
  `var` never.
- `async` / `await` over raw `.then()` chains; never mix
  callbacks and promises in the same flow.
- TypeScript: `strict: true` in `tsconfig.json` (enables
  `noImplicitAny`, `strictNullChecks`, etc.). Don't loosen
  per-file without a reason comment.
- Prefer `unknown` over `any` for untyped boundaries; narrow
  with type guards.
- `=== ` / `!==` over `==` / `!=`; the ESLint `eqeqeq` rule
  enforces it.
- Avoid default exports in libraries — named exports give
  better tooling support and refactorability.
- Catch specific error types; rethrow with context rather
  than swallowing.

## Testing

Pick one framework per project and stick with it — common
choices: **vitest** (modern, vite-native), **jest** (mature),
**node:test** (built-in, zero-dep). File convention:

    tests/unit/<module>.test.ts
    tests/sit/<scenario>.test.ts

Or, equivalently, co-located `src/<module>.test.ts` if the
framework supports it (vitest does by default). Pick one
layout — don't mix.

Test functions: `describe(...)` / `it(...)` / `test(...)`,
the de-facto vocabulary across frameworks. Fixtures via
`beforeEach` / `afterEach`; shared helpers under
`tests/helpers/`.

For browser-driven SIT, Playwright is the common choice and
its `playwright.config.ts` is where projects/grep filters
live.

The cross-walk to `.repo/project/skills/testing.md`:
`tests/unit/` is the unit layer, `tests/sit/` is SIT (podman
fixtures alongside in `tests/sit/podman/` when the SIT
involves backing services), `tests/pit/` is PIT.

## Reading

- *MDN JavaScript reference* —
  <https://developer.mozilla.org/en-US/docs/Web/JavaScript>
- *TypeScript handbook* —
  <https://www.typescriptlang.org/docs/handbook/>
- *Airbnb JavaScript Style Guide* —
  <https://github.com/airbnb/javascript> (common baseline;
  many projects diverge but it's the reference point)
- *typescript-eslint rules* —
  <https://typescript-eslint.io/rules/>
- Per-package `CLAUDE.md`, `eslint.config.js`, and
  `tsconfig.json` (the explicit known-debt list and idiom
  waivers, with reasons)
