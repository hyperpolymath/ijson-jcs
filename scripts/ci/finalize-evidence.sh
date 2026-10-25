#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Aggregate only receipts from commands that actually ran in THIS job.
set -euo pipefail
cargo build --locked --bin ijson-jcs
export IJSON_JCS=$PWD/target/debug/ijson-jcs
if [[ -x "$IJSON_JCS.exe" ]]; then IJSON_JCS=$IJSON_JCS.exe; fi
receipts=(target/ci/*.receipt.raw)
[[ -s "${receipts[0]}" ]] || { echo 'No execution receipts: not evidence' >&2; exit 2; }
jq -s -e 'length > 0 and all(.[]; .schema_version == 1 and .provenance == "Unproven" and (.argv | length > 0) and (.exit_code | type == "number"))' "${receipts[@]}" >/dev/null
jq -s '{schema_version:1,subject:"ijson-jcs",provenance:"Unproven",executions:.}' "${receipts[@]}" > target/ci/executions.raw
bash scripts/ci/canonical-report.sh target/ci/executions.raw target/ci/executions.json
sha256sum Cargo.lock fuzz/Cargo.lock .ci/standards.sha256 .ci/pins.sh rust-toolchain.toml verification/suites.toml > target/ci/inputs.sha256
# Source bytes, not a mutable branch label. Dirty trees are explicitly visible.
git rev-parse HEAD > target/ci/source-revision.txt
git status --porcelain > target/ci/source-delta.txt
