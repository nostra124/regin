# regin developer targets.
# COVERAGE_MIN*: enforced line-coverage floors (FEAT-058 / FEAT-075) — a
# no-regression ratchet, each set just below its ACTUAL measured coverage
# (verified locally with `cargo llvm-cov`) so `make coverage` genuinely
# passes today and a real regression genuinely fails it. Workspace-wide
# AND per-crate: a per-crate floor exists so one crate can't hide behind
# another's coverage (e.g. regind's binary glue hiding behind regin-core's
# well-tested library). Ratchet up as remaining gaps close — literal 100%
# with no exclusions is the 0.5.0 exit-criterion target, but is not yet
# reachable without covering transport.rs's systemd/process-spawn paths,
# reflect.rs's live-LLM curate_once, and regind/regin-cli's remaining CLI
# glue (see .repo/dvalin/notes.md, FEAT-075 entry, for the full baseline).
# No CI workflow enforces this (removed — see notes.md); run `make coverage`
# locally before pushing.
COVERAGE_MIN ?= 85
COVERAGE_MIN_REGIN_CORE ?= 90
COVERAGE_MIN_REGIND ?= 80
COVERAGE_MIN_REGIN_CLI ?= 75

.PHONY: build test clippy packages pit coverage all

all: build test clippy

build:
	cargo build --workspace

test:
	cargo test --workspace

clippy:
	cargo clippy --workspace --all-targets

# Build deb/rpm/apk into dist/ via the nfpm recipe (FEAT-053).
packages:
	packaging/build.sh

# Per-format install PITs in podman (FEAT-054 / RULE-003/004). Needs the packages.
pit: packages
	bats tests/pit/

# Coverage gate (FEAT-058 / FEAT-075): collects profile data once, then
# evaluates it against the workspace-wide floor AND each crate's own floor —
# any one of the four failing fails the target.
coverage:
	cargo llvm-cov --workspace --no-report
	cargo llvm-cov report --fail-under-lines $(COVERAGE_MIN) --summary-only
	cargo llvm-cov report -p regin-core --fail-under-lines $(COVERAGE_MIN_REGIN_CORE) --summary-only
	cargo llvm-cov report -p regind --fail-under-lines $(COVERAGE_MIN_REGIND) --summary-only
	cargo llvm-cov report -p regin-cli --fail-under-lines $(COVERAGE_MIN_REGIN_CLI) --summary-only
