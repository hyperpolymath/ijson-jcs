#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# detect.sh — has metadatastician/standards moved since the pinned revision?
#
# WHY: "track standards" is a comparison, not a checkout. This script compares
# the commit in .ci/pins.sh with the commit the upstream branch points at, and
# reports drift without changing anything. Nothing here follows `main` at run
# time: the answer is only ever "same" (0) or "drift" (1), and the drift case
# becomes a reviewed proposal, never a silent upgrade.
#
# USAGE
#   detect.sh [--root DIR] [--head SHA] [--remote URL] [--format kv|json]
#
#   --root DIR    repository root holding .ci/ (default: this checkout)
#   --head SHA    assert this commit is the upstream head, without the network
#                 (used by tests and by a caller that already resolved it)
#   --remote URL  git URL to ask (default: https://github.com/<STANDARDS_REPOSITORY>.git)
#   --format      kv (default) or json
#
# OUTPUT (stdout)
#   pin=<40-hex>            repository=metadatastician/standards
#   head=<40-hex>           drift=true|false
#
# EXIT  0 no drift; 1 drift; 2 cannot determine (no git, unreachable remote,
#       unparseable answer). A gate that cannot look must not report a pass.
set -euo pipefail
IS_PROGRAM=detect.sh
# shellcheck source=scripts/standards/lib.sh
here="$(cd -- "${0%/*}" && pwd)"
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# EXIT/p' "$0" | sed 's/^# \{0,1\}//'
}

root="$(ijson_standards_root)"
head=""
remote=""
format=kv

while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="${2:?--root needs a directory}"; shift 2 ;;
    --head) head="${2:?--head needs a commit}"; shift 2 ;;
    --remote) remote="${2:?--remote needs a URL}"; shift 2 ;;
    --format) format="${2:?--format needs kv or json}"; shift 2 ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    *) ijson_die "$IS_EXIT_CANNOT" "unknown option $1" ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"

root="$(cd -- "$root" && pwd)"
pin="$(ijson_pin "$root" STANDARDS_REV)"
repository="$(ijson_pin "$root" STANDARDS_REPOSITORY)"
ijson_is_revision "$pin" || ijson_die "$IS_EXIT_CANNOT" "pinned revision is not a full commit: $pin"

if [ -z "$head" ]; then
  [ -n "$remote" ] || remote="https://github.com/$repository.git"
  ijson_require git "a drift check needs git to ask the remote"
  answer="$(git ls-remote --heads "$remote" main 2>/dev/null)" ||
    ijson_die "$IS_EXIT_CANNOT" "cannot query $remote"
  head="$(printf '%s\n' "$answer" | awk 'NR==1 {print $1}')"
  [ -n "$head" ] || ijson_die "$IS_EXIT_CANNOT" "$remote has no main branch"
fi
ijson_is_revision "$head" || ijson_die "$IS_EXIT_CANNOT" "upstream head is not a full commit: $head"

if [ "$head" = "$pin" ]; then
  drift=false
  status="$IS_EXIT_OK"
else
  drift=true
  status="$IS_EXIT_FINDING"
fi

case "$format" in
  kv)
    printf 'repository=%s\npin=%s\nhead=%s\ndrift=%s\n' "$repository" "$pin" "$head" "$drift"
    ;;
  json)
    printf '{"drift":%s,"head":"%s","pin":"%s","repository":"%s"}\n' \
      "$drift" "$head" "$pin" "$repository"
    ;;
  *) ijson_die "$IS_EXIT_CANNOT" "--format must be kv or json" ;;
esac
exit "$status"
