#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
source .ci/pins.sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version "$CARGO_LLVM_COV_VERSION" --locked
mkdir -p target/ci
# Exclude standalone harness/example sources. Inline unit tests in src remain
# visible: this is source-file line coverage, not a proof of production paths.
cargo llvm-cov --locked --all-features --lcov --output-path target/ci/lcov.info \
  --ignore-filename-regex '(/tests/|/benches/|/examples/)' --fail-under-lines 85
cargo llvm-cov report --json --summary-only --output-path target/ci/coverage.raw \
  --ignore-filename-regex '(/tests/|/benches/|/examples/)'
cargo build --locked --bin ijson-jcs
bash scripts/ci/canonical-report.sh target/ci/coverage.raw target/ci/coverage.json
jq -e '.type == "llvm.coverage.json.export" and (.data | length) == 1 and .data[0].totals.lines.count > 0 and .data[0].totals.lines.percent >= 85' target/ci/coverage.json >/dev/null
printf '::notice title=Instrumented source coverage::%s\n' "$(jq -c '.data[0].totals | {lines,functions,regions}' target/ci/coverage.json)"
