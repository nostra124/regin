#!/usr/bin/env bats
# FEAT-054: install the .rpm in a clean Fedora container and verify regin works.

setup() {
  PKG=$(ls "$BATS_TEST_DIRNAME"/../../dist/regin*.rpm 2>/dev/null | head -1)
  [ -n "$PKG" ] || skip "no .rpm in dist/ (run packaging/build.sh first)"
}

@test "rpm: installs and regin runs on fedora:latest" {
  run podman run --rm -v "$BATS_TEST_DIRNAME/../../dist:/dist:ro" fedora:latest sh -c '
    set -e
    rpm -Uvh /dist/regin*.rpm
    regin --version
    regin --help >/dev/null
    test -f /usr/share/man/man1/regin.1
    test -d /usr/share/regin/operator-skills
  '
  [ "$status" -eq 0 ]
}
