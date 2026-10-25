#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Silence/firing controls: fixture failures are expected, not failed CI receipts.
set -euo pipefail
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
export IJSON_EVIDENCE_DIR=$work/evidence
expect() {
  local want=$1; shift
  local actual=0
  "$@" > "$work/control.log" 2>&1 || actual=$?
  if (( actual != want )); then cat "$work/control.log"; echo "expected $want got $actual" >&2; exit 1; fi
}
expect 0 bash scripts/ci/run.sh control-pass true
expect 7 bash scripts/ci/run.sh control-fail bash -c 'exit 7'
expect 2 bash scripts/ci/run.sh control-missing
expect 2 bash scripts/ci/run.sh '../escape' true
expect 127 bash scripts/ci/run.sh control-absent ijson-deliberately-missing-command
expect 124 bash scripts/ci/run.sh control-timeout timeout 0.1 bash -c 'sleep 2'
printf 'not a directory' > "$work/file"
expect 1 env IJSON_EVIDENCE_DIR="$work/file" bash scripts/ci/run.sh control-bad-output true
jq -e '.exit_code == 0 and .argv == ["true"] and .provenance == "Unproven"' "$IJSON_EVIDENCE_DIR/control-pass.receipt.raw" >/dev/null
jq -e '.exit_code == 7 and .status == "failed"' "$IJSON_EVIDENCE_DIR/control-fail.receipt.raw" >/dev/null
[[ ! -e "$IJSON_EVIDENCE_DIR/control-missing.receipt.raw" ]]
printf 'CI wrapper controls passed (clean, failure, missing command/tool, timeout, output failure)\n'
