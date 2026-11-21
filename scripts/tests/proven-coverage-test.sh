#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0 AND CC-BY-SA-4.0
# Code is MPL-2.0; prose is CC-BY-SA-4.0.
#
# Firing and silence fixtures for scripts/proven-coverage.sh. Needs the lcov
# from one full gate run (target/proven-coverage.lcov); every case then
# re-judges that lcov against a mutated copy of the exclusions file.
# The awk programs below are single-quoted on purpose: awk, not the shell,
# expands $N.
# shellcheck disable=SC2016
set -euo pipefail
cd "$(dirname "$0")/../.."
gate=scripts/proven-coverage.sh
real=.proven/coverage-exclusions.tsv
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

# case NAME WANT_EXIT WANT_PHRASE AWK_PROGRAM (rewrites the exclusions file).
case_() {
  local name=$1 want=$2 phrase=$3 program=$4 got out
  awk -F'\t' -v OFS='\t' "$program" "$real" >"$work/$name.tsv"
  set +e
  out=$(PROVEN_EXCLUSIONS="$work/$name.tsv" bash "$gate" --lcov-only-check 2>&1)
  got=$?
  set -e
  if [[ $got != "$want" || $out != *"$phrase"* ]]; then
    echo "FAIL $name: exit $got (want $want), output: $out" >&2
    fail=1
  else
    echo "ok $name"
  fi
}

case_ silence 0 "SUMMARY" '{ print }'
case_ stale 1 "STALE exclusion" '$2 == "145" { $3 = "0000000000000000000000000000000000000000000000000000000000000000" } { print }'
case_ uncovered 1 "UNCOVERED src/strict.rs:187" '!($1 == "src/strict.rs" && $2 == "187") { print }'
case_ unused 1 "UNUSED-EXCLUSION src/lib.rs:999" '{ print } END { print "src/lib.rs", "999", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "unreachable", "past end of file" }'
case_ malformed 1 "MALFORMED exclusion" '{ print } END { print "src/lib.rs", "1", "x", "maybe", "" }'
exit "$fail"
