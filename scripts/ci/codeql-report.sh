#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
bash scripts/tests/sarif-gate.sh
shopt -s nullglob
reports=(target/ci/codeql/*.sarif)
(( ${#reports[@]} > 0 )) || { echo 'CodeQL produced no SARIF' >&2; exit 2; }
cargo build --locked --bin ijson-jcs
status=0
for report in "${reports[@]}"; do
  bash scripts/ci/canonical-report.sh "$report" "${report%.sarif}.json"
  jq -e -f scripts/ci/sarif-gate.jq "${report%.sarif}.json" >/dev/null || status=$?
done
exit "$status"
