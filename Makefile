# regin developer targets.
# COVERAGE_MIN: the enforced line-coverage floor (FEAT-058) — a no-regression
# ratchet. It sits just below current workspace coverage (the daemon binary and
# tool I/O are still largely untested) and ratchets up toward the 0.5.0 exit
# criterion of 100 as those gaps are closed.
COVERAGE_MIN ?= 55

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

# Coverage gate (FEAT-058): fails under the COVERAGE_MIN line threshold.
coverage:
	cargo llvm-cov --workspace --fail-under-lines $(COVERAGE_MIN) --summary-only
