#!/bin/sh
# regin installer (FEAT-056). Detects the distro's package format, fetches the
# matching package from the latest GitHub release (FEAT-059), installs it, and
# enables the per-user lingering service. Idempotent: safe to re-run (upgrades in
# place). PIT-tested per format by FEAT-054.
#
#   curl -fsSL https://raw.githubusercontent.com/nostra124/regin/main/assets/install.sh | sh
#
set -eu

REPO="${REGIN_REPO:-nostra124/regin}"
API="https://api.github.com/repos/${REPO}/releases/latest"

err() { echo "regin-install: $*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- detect the package format for this distro ---------------------------------
detect_format() {
  if have dpkg; then echo deb; return; fi
  if have rpm; then echo rpm; return; fi
  if have apk; then echo apk; return; fi
  if [ -f /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    case "${ID:-}${ID_LIKE:-}" in
      *debian*|*ubuntu*) echo deb; return ;;
      *rhel*|*fedora*|*centos*|*suse*) echo rpm; return ;;
      *alpine*) echo apk; return ;;
    esac
  fi
  err "unsupported platform: no dpkg/rpm/apk and unrecognized /etc/os-release"
}

# --- pick a downloader --------------------------------------------------------
fetch() { # fetch URL OUT
  if have curl; then curl -fsSL "$1" -o "$2"
  elif have wget; then wget -qO "$2" "$1"
  else err "need curl or wget to download"
  fi
}
fetch_stdout() {
  if have curl; then curl -fsSL "$1"
  elif have wget; then wget -qO- "$1"
  else err "need curl or wget to download"
  fi
}

# --- resolve the latest release asset for our format --------------------------
asset_url() { # asset_url FORMAT
  fmt="$1"
  fetch_stdout "$API" \
    | grep -o "\"browser_download_url\": *\"[^\"]*\\.${fmt}\"" \
    | head -1 \
    | sed 's/.*"\(https[^"]*\)"/\1/'
}

install_pkg() { # install_pkg FORMAT FILE
  case "$1" in
    deb) if [ "$(id -u)" = 0 ]; then dpkg -i "$2" || apt-get -fy install; else sudo dpkg -i "$2" || sudo apt-get -fy install; fi ;;
    rpm) if [ "$(id -u)" = 0 ]; then rpm -Uvh --replacepkgs "$2"; else sudo rpm -Uvh --replacepkgs "$2"; fi ;;
    apk) if [ "$(id -u)" = 0 ]; then apk add --allow-untrusted "$2"; else sudo apk add --allow-untrusted "$2"; fi ;;
    *) err "unknown format $1" ;;
  esac
}

main() {
  fmt=$(detect_format)
  echo "regin-install: detected package format: ${fmt}"

  url=$(asset_url "$fmt") || true
  [ -n "${url:-}" ] || err "no .${fmt} asset in the latest release of ${REPO}"

  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  pkg="${tmp}/regin.${fmt}"
  echo "regin-install: downloading ${url}"
  fetch "$url" "$pkg"

  echo "regin-install: installing"
  install_pkg "$fmt" "$pkg"

  # Enable the per-user lingering service (idempotent). Best-effort.
  if have systemctl && [ "$(id -u)" != 0 ]; then
    regin config set daemon.enabled true >/dev/null 2>&1 || true
  fi

  echo "regin-install: done. Run 'regin chat' to start."
}

main "$@"
