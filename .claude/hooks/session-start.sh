#!/bin/bash
# SessionStart hook — ensure podman and bats are available.
#
# This project's test tiers depend on two system binaries that Claude Code
# web sandboxes do not ship with:
#   - podman : PIT suites run inside podman containers (never docker).
#   - bats   : SIT suites are bats (Bash Automated Testing System) cases.
# Installing both here makes `make check-sit` / `make check-pit` work in a
# fresh web session. Idempotent and non-interactive.
set -euo pipefail

# Only run in Claude Code on the web; local machines manage their own tools.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

SUDO=""
[ "$(id -u)" -ne 0 ] && SUDO="sudo"

# Collect only the missing packages so we apt-get only when there is work.
pkgs=()
command -v podman >/dev/null 2>&1 || pkgs+=(podman)
command -v bats   >/dev/null 2>&1 || pkgs+=(bats)

if [ ${#pkgs[@]} -eq 0 ]; then
  echo "session-start: podman ($(podman --version)) and bats ($(bats --version)) already installed" >&2
  exit 0
fi

export DEBIAN_FRONTEND=noninteractive
echo "session-start: installing ${pkgs[*]}..." >&2
$SUDO apt-get update -y
$SUDO apt-get install -y "${pkgs[@]}"

echo "session-start: ready — podman $(podman --version), bats $(bats --version)" >&2
