#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Every shard uses identical scope/ordering, with an unmutated baseline.
set -euo pipefail
source .ci/pins.sh
shard=${1:-0/1}
[[ "$shard" =~ ^(0|[1-9][0-9]*)/[1-9][0-9]*$ ]] || { echo 'shard must be k/n' >&2; exit 2; }
index=${shard%/*}; total=${shard#*/}
(( index < total && total <= 64 )) || { echo 'invalid shard bounds' >&2; exit 2; }
cargo install cargo-mutants --version "$CARGO_MUTANTS_VERSION" --locked
mkdir -p target/ci
scope=(--file 'src/**/*.rs' --file 'src/*.rs' --no-shuffle --sharding round-robin --cargo-arg=--locked)
cargo mutants "${scope[@]}" --list --json > target/ci/mutation-inventory.raw
cargo mutants "${scope[@]}" --shard "$shard" --list --json > target/ci/mutation-selected.raw
expected=$(jq -er 'length | select(. > 0)' target/ci/mutation-selected.raw)
inventory=$(sha256sum target/ci/mutation-inventory.raw); inventory=${inventory%% *}
status=0
# Use a measured baseline multiplier, not a long fixed timeout that lets
# infinite-loop mutants consume the entire job. Timeouts STILL fail the gate.
cargo mutants "${scope[@]}" --shard "$shard" --jobs 1 \
  --timeout-multiplier 5 --minimum-test-timeout 20 --build-timeout 240 \
  --output target/ci/mutation || status=$?
report=target/ci/mutation/mutants.out/outcomes.json
[[ -s "$report" ]] || { echo 'mutation run produced no outcomes' >&2; exit 2; }
cargo build --locked --bin ijson-jcs
mv "$report" "${report%.json}.raw"
bash scripts/ci/canonical-report.sh "${report%.json}.raw" "$report"
jq --arg source "$(git rev-parse HEAD)" --arg inventory "$inventory" \
  --argjson index "$index" --argjson total "$total" --argjson expected "$expected" \
  --argjson inventory_count "$(jq length target/ci/mutation-inventory.raw)" \
  '{schema_version:1,provenance:"Unproven",source:$source,index:$index,total_shards:$total,
    inventory_sha256:$inventory,total_inventory:$inventory_count,expected:$expected,
    total_mutants,caught,missed,timeout,unviable}' "$report" > target/ci/mutation-shard.raw
bash scripts/ci/canonical-report.sh target/ci/mutation-shard.raw target/ci/mutation-shard.json
printf '::notice title=Mutation shard::%s\n' "$(cat target/ci/mutation-shard.json)"
# Keep full diagnostics but do not flood Checks' ten-annotations-per-step cap.
jq -e --argjson expected "$expected" '.total_mutants == $expected and .caught > 0 and .missed == 0 and .timeout == 0' "$report" >/dev/null
exit "$status"
