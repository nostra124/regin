#!/bin/sh
# Regenerable package build (FEAT-053): release binaries + fresh man pages,
# version stamped from Cargo.toml, then nfpm produces deb + rpm + apk into dist/.
# No build artifacts are committed (RULE-011); run this to (re)create packages.
#
#   packaging/build.sh
#
set -eu
cd "$(dirname "$0")/.."

# Single source of truth for the version: the workspace Cargo.toml.
VERSION=$(grep -m1 '^version = ' Cargo.toml | sed 's/^version = "\(.*\)"/\1/')
[ -n "$VERSION" ] || { echo "could not read version from Cargo.toml" >&2; exit 1; }
export VERSION
echo "building regin packages for version ${VERSION}"

cargo build --release
./target/release/regin gen-man man/

command -v nfpm >/dev/null 2>&1 || {
  echo "nfpm not found — install from https://nfpm.goreleaser.com" >&2
  exit 1
}

mkdir -p dist
for fmt in deb rpm apk; do
  echo "packaging ${fmt}"
  nfpm package -f packaging/nfpm.yaml -p "$fmt" -t dist/
done
echo "done: $(ls dist/)"
