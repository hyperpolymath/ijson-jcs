#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Scheduled/manual empirical test of production Rust. Surviving mutants fail.
set -euo pipefail
source .ci/pins.sh
cargo install cargo-mutants --version "$CARGO_MUTANTS_VERSION" --locked
mkdir -p target/ci
status=0
cargo mutants --file 'src/**/*.rs' --file 'src/*.rs' --jobs 2 \
  --timeout 90 --build-timeout 240 --cargo-arg=--locked \
  --output target/ci/mutation || status=$?
report=target/ci/mutation/mutants.out/outcomes.json
[[ -s "$report" ]] || { echo 'mutation run produced no outcomes' >&2; exit 2; }
cargo build --locked --bin ijson-jcs
mv "$report" "${report%.json}.raw"
bash scripts/ci/canonical-report.sh "${report%.json}.raw" "$report"
jq '{total_mutants,caught,missed,timeout,unviable}' "$report"
# A successful process with zero injected/exercised faults is not evidence.
jq -e '.total_mutants > 0 and .caught > 0 and .missed == 0 and .timeout == 0' "$report" >/dev/null
exit "$status"
