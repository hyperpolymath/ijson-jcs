#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
source .ci/pins.sh
proven=$(bash scripts/ci/fetch-source.sh proven-tests-and-benches hyperpolymath/proven-tests-and-benches "$PROVEN_REV")
# This is the upstream tool's own fault-injection suite, not a copied reimplementation.
bash "$proven/scripts/test/check-bench-regression.test.sh"
mkdir -p target/ci
export IJSON_BENCH_OUTPUT=$PWD/target/ci/proven-bench.json
cargo bench --locked --bench jcs
jq -L "$proven/scripts" -e -s 'include "bench-stats";
  length == 1 and (.[0] | (bench_errors | length) == 0 and (checksum_errors | length) == 0)
' "$IJSON_BENCH_OUTPUT" >/dev/null
cargo build --locked --bin ijson-jcs
IJSON_JCS=$PWD/target/debug/ijson-jcs bash scripts/check-estate-json.sh "$IJSON_BENCH_OUTPUT"
if [[ -e .ci/bench-baseline.json ]]; then
  bash "$proven/scripts/check-bench-regression.sh" "$IJSON_BENCH_OUTPUT" .ci/bench-baseline.json
else
  printf '%s\n' '::notice title=Benchmark baseline::BOOTSTRAPPING: measured workloads/checksums passed; timing regression NOT RUN. Requires at least ten accepted main-run receipts. No synthetic baseline is committed.'
fi
