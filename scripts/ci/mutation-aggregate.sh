#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Completeness is separate from correctness: all shards must be present.
set -euo pipefail
shopt -s nullglob
reports=(target/ci/shards/evidence-mutation-*/mutation-shard.json)
(( ${#reports[@]} == 16 )) || { echo 'incomplete mutation campaign: expected 16 shard receipts' >&2; exit 2; }
jq -s -e --arg source "$(git rev-parse HEAD)" '
  length == 16 and (map(.index) | sort) == [range(0;16)] and
  all(.[]; .schema_version == 1 and .source == $source and .provenance == "Unproven" and
    .total_shards == 16 and .expected > 0 and .total_mutants == .expected) and
  (map(.inventory_sha256) | unique | length) == 1 and
  (map(.total_inventory) | unique | length) == 1 and
  (map(.total_mutants) | add) == .[0].total_inventory
' "${reports[@]}" >/dev/null
jq -s '{schema_version:1,provenance:"Unproven",source:.[0].source,shards:.,
  total_mutants:(map(.total_mutants)|add),caught:(map(.caught)|add),
  missed:(map(.missed)|add),timeout:(map(.timeout)|add),unviable:(map(.unviable)|add)}' \
  "${reports[@]}" > target/ci/mutation-complete.raw
cargo build --locked --bin ijson-jcs
bash scripts/ci/canonical-report.sh target/ci/mutation-complete.raw target/ci/mutation-complete.json
jq '{source,total_mutants,caught,missed,timeout,unviable}' target/ci/mutation-complete.json
jq -e '.caught > 0 and .missed == 0 and .timeout == 0' target/ci/mutation-complete.json >/dev/null
