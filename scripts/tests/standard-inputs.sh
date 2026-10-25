#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Committed clean/firing recipes; no network and no fabricated standards pin.
set -euo pipefail
script=$PWD/scripts/ci/standard-inputs.sh
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
mkdir -p "$work/source space/policy"
printf 'accepted normative input\n' > "$work/source space/policy/contract.adoc"
(cd "$work/source space" && sha256sum policy/contract.adoc) > "$work/inputs.sha256"
expect() {
  local want=$1; shift
  local got=0; "$@" > "$work/log" 2>&1 || got=$?
  if (( got != want )); then cat "$work/log"; echo "wanted $want, got $got" >&2; exit 1; fi
}
expect 0 bash "$script" "$work/source space" "$work/inputs.sha256"
printf 'incompatible policy change\n' > "$work/source space/policy/contract.adoc"
expect 1 bash "$script" "$work/source space" "$work/inputs.sha256"
rm "$work/source space/policy/contract.adoc"
expect 2 bash "$script" "$work/source space" "$work/inputs.sha256"
expect 2 bash "$script" "$work/missing" "$work/inputs.sha256"
expect 2 bash "$script" "$work/source space" "$work/missing"
printf '' > "$work/empty"
expect 2 bash "$script" "$work/source space" "$work/empty"
printf '%064d  ../escape\n' 0 > "$work/unsafe"
expect 2 bash "$script" "$work/source space" "$work/unsafe"
echo 'Standards inputs controls passed: clean, drifted, missing, empty, traversal'
