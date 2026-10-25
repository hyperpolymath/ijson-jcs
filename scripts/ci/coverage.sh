#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
source .ci/pins.sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version "$CARGO_LLVM_COV_VERSION" --locked
mkdir -p target/ci
cargo llvm-cov --locked --all-features --lcov --output-path target/ci/lcov.info --fail-under-lines 85
