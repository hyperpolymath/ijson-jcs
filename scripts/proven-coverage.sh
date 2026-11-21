#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0 AND CC-BY-SA-4.0
# Code is MPL-2.0; prose is CC-BY-SA-4.0.
#
# Whole-crate counted-coverage gate. Runs only the counted suites (seeded
# properties and sha256-pinned vectors) under cargo-llvm-cov, then fails
# unless every instrumented line is covered or listed, with a current source
# hash, in .proven/coverage-exclusions.tsv. Lines only: stable Rust gives no
# branch data.
set -euo pipefail
cd "$(dirname "$0")/.."

counted=(--test properties --test proven_properties --test proven_vectors --test proven_mutation_kills)
exclusions=${PROVEN_EXCLUSIONS:-.proven/coverage-exclusions.tsv}
lcov=${PROVEN_LCOV:-target/proven-coverage.lcov}

if [[ ${1:-} != --lcov-only-check ]]; then
  cargo llvm-cov clean --workspace
  cargo llvm-cov --locked --lcov --output-path "$lcov" "${counted[@]}"
fi

status=0
excluded=$(mktemp)
trap 'rm -f "$excluded"' EXIT

# Expand each exclusion to path:line, refusing stale or malformed entries.
while IFS=$'\t' read -r path lines digest kind reason; do
  [[ -z $path || $path == \#* ]] && continue
  if [[ -z $reason || ! $kind =~ ^(unreachable|environment)$ ]]; then
    echo "MALFORMED exclusion: $path $lines" >&2; status=1; continue
  fi
  first=${lines%-*}; last=${lines#*-}
  actual=$(sed -n "${first},${last}p" "$path" | head -c -1 | sha256sum | cut -d' ' -f1)
  if [[ $actual != "$digest" ]]; then
    echo "STALE exclusion (source changed): $path:$lines" >&2; status=1; continue
  fi
  for ((n = first; n <= last; n++)); do echo "$path:$n"; done >>"$excluded"
done <"$exclusions"

# Every zero-hit line must be excluded; every exclusion must be a real gap.
report=$(awk -v root="$PWD/" -v ex="$excluded" '
  BEGIN { while ((getline line < ex) > 0) want[line] = 1 }
  /^SF:/ { file = substr($0, 4); sub(root, "", file) }
  /^DA:/ {
    split(substr($0, 4), f, ","); key = file ":" f[1]; total++
    if (f[2] > 0) { covered++; if (key in want) print "COVERED-BUT-EXCLUDED " key }
    else if (key in want) { excl++; seen[key] = 1 }
    else print "UNCOVERED " key
  }
  END {
    for (k in want) if (!(k in seen)) print "UNUSED-EXCLUSION " k
    printf "SUMMARY lines=%d covered=%d excluded=%d\n", total, covered, excl
  }' "$lcov")
echo "$report"
if grep -qv '^SUMMARY' <<<"$report"; then status=1; fi
exit "$status"
