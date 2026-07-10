# third_party/

Vendored artifacts from regin's two upstream dependencies, pulled in as
files only — no build/link wiring yet. `regin-core` does not consume
either of these; this is a staging drop for a future FFI-integration
ticket.

## libnorn

- Source: [nostra124/norn](https://github.com/nostra124/norn) GitHub Release
  [`v0.18.0`](https://github.com/nostra124/norn/releases/tag/v0.18.0)
  (commit `d61dab10a109e21f1d37bab7cd06b6bcb7a2ea6b`), published 2026-07-09.
- Contents: the two Linux/amd64 `.deb` release assets, taken verbatim
  (sha256 verified against the release manifest):
  - `libnorn/libnorn_0.18.0_amd64.deb` — runtime shared library.
  - `libnorn/libnorn-dev_0.18.0_amd64.deb` — headers + dev symlinks for
    linking against libnorn.
- Not included: the `norn` CLI/`nornd` daemon `.deb` (regin only needs the
  library), and non-Linux assets (regin targets Linux only per
  `.repo/project/profile.md` §7).

## libmimir

- Source: [nostra124/mimir](https://github.com/nostra124/mimir), directory
  `libmimir/` at tag [`v0.12.0`](https://github.com/nostra124/mimir/tree/v0.12.0/libmimir)
  (commit `050e21ff781019695caade9d66a7bf390dbb652f`), 2026-06-24.
- **No GitHub Release exists for mimir yet** (`v0.12.0` is a git tag only,
  `releases/latest` 404s) — this is the closest equivalent to "the
  release" available today. Re-vendor from an actual Release once mimir
  cuts one.
- Contents: the full `libmimir/` C SDK source tree as committed at that
  tag — `include/` (`mimir.h`, `mimir_curl.h`), `src/` (`mimir.c`,
  `mimir_curl.c`), `test/`, and its own `README.md`/`.gitignore`, copied
  verbatim (no modifications).

## Updating

Re-run the same fetch (release assets by tag for norn; `libmimir/` tree at
a tag/release for mimir) and replace the directory contents wholesale —
don't hand-edit vendored files in place.
