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
    dpkg -i /dist/regin_*.deb
    regin --version
    regin --help >/dev/null
    test -d /usr/share/regin/operator-skills
    # slim Debian images path-exclude /usr/share/man on install, so verify the
    # package *ships* the man page rather than that it survived the exclusion.
    dpkg-deb -c /dist/regin_*.deb | grep -q "usr/share/man/man1/regin.1"
  '
  [ "$status" -eq 0 ]
}
