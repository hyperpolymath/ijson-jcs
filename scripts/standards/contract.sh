#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# contract.sh — assert that the pinned policies still say what this
# implementation is tested against.
#
# WHY: a new standards revision can keep every filename and still move the
# goalposts: drop the "exactly one line feed" sentence, rename rule J-2, or
# remove the `--enforce` mode this repository's gate calls. A hash mismatch
# cannot catch that, because the new file is a perfectly valid *new* policy.
# So each normative dependency is recorded as a literal in
# .ci/standards-contract.tsv, and a revision that no longer contains it is an
# incompatible policy change: exit 3, and the proposal stops for a human.
#
# USAGE
#   contract.sh [--root DIR] [--contract FILE] [--tree DIR] [--rev SHA] [--list]
#
#   --root DIR       repository root holding .ci/ (default: this checkout)
#   --contract FILE  contract to assert (default: .ci/standards-contract.tsv)
#   --tree DIR       already-materialised standards tree
#   --rev SHA        revision to fetch when --tree is not given
#   --list           print the assertions and exit without a tree
#
# EXIT  0 every assertion holds; 2 cannot run; 3 a pinned file is missing or an
#       assertion no longer holds (an incompatible policy change).
set -euo pipefail
IS_PROGRAM=contract.sh
# shellcheck source=scripts/standards/lib.sh
here="$(cd -- "${0%/*}" && pwd)"
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# EXIT/p' "$0" | sed 's/^# \{0,1\}//'
}

root="$(ijson_standards_root)"
contract=""
tree=""
rev=""
list=0

while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="${2:?--root needs a directory}"; shift 2 ;;
    --contract) contract="${2:?--contract needs a file}"; shift 2 ;;
    --tree) tree="${2:?--tree needs a directory}"; shift 2 ;;
    --rev) rev="${2:?--rev needs a commit}"; shift 2 ;;
    --list) list=1; shift ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    *) ijson_die "$IS_EXIT_CANNOT" "unknown option $1" ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"

root="$(cd -- "$root" && pwd)"
[ -n "$contract" ] || contract="$root/.ci/standards-contract.tsv"
[ -f "$contract" ] || ijson_die "$IS_EXIT_CANNOT" "contract not found: $contract"
ijson_validate_contract "$contract"

if [ "$list" -eq 1 ]; then
  ijson_contract_entries "$contract" | awk -F'\t' '{printf "%-12s %-46s %s\n", $1, $2, $3}'
  exit "$IS_EXIT_OK"
fi

if [ -z "$tree" ]; then
  args=(--root "$root")
  if [ -n "$rev" ]; then
    args+=(--rev "$rev")
  fi
  tree="$(bash "$here/fetch.sh" "${args[@]}")"
else
  tree="$(cd -- "$tree" && pwd)"
fi
[ -n "$rev" ] || rev="$(ijson_pin "$root" STANDARDS_REV)"

assertions=0
failures=0
files=0
declare -A seen=()
while IFS=$'\t' read -r role path literal; do
  assertions=$((assertions + 1))
  if [ -z "${seen[$path]:-}" ]; then
    seen[$path]=1
    files=$((files + 1))
  fi
  if [ ! -f "$tree/$path" ]; then
    printf 'MISSING %s (%s): the revision does not carry this file\n' "$path" "$role" >&2
    failures=$((failures + 1))
    continue
  fi
  if ! grep -qF -- "$literal" "$tree/$path"; then
    printf 'INCOMPATIBLE POLICY CHANGE %s (%s): no longer contains: %s\n' \
      "$path" "$role" "$literal" >&2
    failures=$((failures + 1))
  fi
done < <(ijson_contract_entries "$contract")

if [ "$failures" -ne 0 ]; then
  ijson_die "$IS_EXIT_WRONG_INPUT" \
    "$failures of $assertions contract assertion(s) failed at $rev; a human must rule on this revision"
fi
[ "$assertions" -gt 0 ] || ijson_die "$IS_EXIT_WRONG_INPUT" "contract $contract asserts nothing"
printf 'contract: %d assertion(s) over %d file(s) hold at %s\n' \
  "$assertions" "$files" "$rev"
