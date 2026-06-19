#!/usr/bin/env bats
# FEAT-054: install the .deb in a clean Debian container and verify regin works
# end-to-end (not just that the package installs). RULE-003 (podman) / RULE-004.

setup() {
  PKG=$(ls "$BATS_TEST_DIRNAME"/../../dist/regin_*.deb 2>/dev/null | head -1)
  [ -n "$PKG" ] || skip "no .deb in dist/ (run packaging/build.sh first)"
}

@test "deb: installs and regin runs on debian:stable-slim" {
  run podman run --rm -v "$BATS_TEST_DIRNAME/../../dist:/dist:ro" debian:stable-slim sh -c '
    set -e
    apt-get update -qq
    apt-get install -y -qq /dist/regin_*.deb
    regin --version
    regin --help >/dev/null
    test -f /usr/share/man/man1/regin.1
    test -d /usr/share/regin/operator-skills
  '
  [ "$status" -eq 0 ]
}
