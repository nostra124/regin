#!/usr/bin/env bats
# FEAT-054: install the .apk in a clean Alpine container and verify regin works.

setup() {
  PKG=$(ls "$BATS_TEST_DIRNAME"/../../dist/regin*.apk 2>/dev/null | head -1)
  [ -n "$PKG" ] || skip "no .apk in dist/ (run packaging/build.sh first)"
}

@test "apk: installs and regin runs on alpine:latest" {
  run podman run --rm -v "$BATS_TEST_DIRNAME/../../dist:/dist:ro" alpine:latest sh -c '
    set -e
    apk add --allow-untrusted /dist/regin*.apk
    regin --version
    regin --help >/dev/null
    test -f /usr/share/man/man1/regin.1
    test -d /usr/share/regin/operator-skills
  '
  [ "$status" -eq 0 ]
}
