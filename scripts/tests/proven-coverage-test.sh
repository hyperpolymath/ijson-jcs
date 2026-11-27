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

# judge NAME WANT_EXIT WANT_PHRASE LCOV EXCLUSIONS [SCRIPT]: one run on a given report.
judge() {
  local name=$1 want=$2 phrase=$3 report=$4 rows=$5 script=${6:-$gate} got out
  set +e
  out=$(PROVEN_LCOV="$report" PROVEN_EXCLUSIONS="$rows" bash "$script" --lcov-only-check 2>&1)
  got=$?
  set -e
  if [[ $got != "$want" || $out != *"$phrase"* ]]; then
    echo "FAIL $name: exit $got (want $want), output: $out" >&2
    fail=1
  else
    echo "ok $name"
  fi
}

# An empty report is a finding, never a clean result.
: >"$work/empty.lcov"; : >"$work/none.tsv"
judge empty-report 1 "EMPTY-REPORT" "$work/empty.lcov" "$work/none.tsv"

# The checkout prefix is removed as literal text: a path holding regex
# metacharacters must still be shown relative to the repository root.
mkdir -p "$work/build[1].x/repo/scripts"
cp "$gate" "$work/build[1].x/repo/scripts/proven-coverage.sh"
printf 'SF:%s/build[1].x/repo/src/a.rs\nDA:1,0\nend_of_record\n' "$work" >"$work/meta.lcov"
judge literal-prefix 1 "UNCOVERED src/a.rs:1" "$work/meta.lcov" "$work/none.tsv" "$work/build[1].x/repo/scripts/proven-coverage.sh"
exit "$fail"
