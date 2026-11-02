#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# publish-kyq-drop.sh — propose this repository's contract drop to
# metadatastician/kyq as one reviewable pull request.
#
# WHY a pull request and not a push: kyq consumes this contract, so a change to
# it is a change to a consumer. The drop is copied into kyq as
# `interop/ijson-jcs/`, the pull request body names the ijson-jcs revision and
# the receipt, and a human merges it. Nothing here writes to kyq's default
# branch, and nothing here claims kyq is ready: verifying these vectors is one
# input to kyq's readiness, not the verdict.
#
# USAGE
#   publish-kyq-drop.sh [--root DIR] [--kyq-repo OWNER/NAME] [--dry-run]
#
# ENVIRONMENT
#   KYQ_SYNC_TOKEN   token that may open pull requests in the kyq repository
#                    (the authorised estate mechanism). Required unless
#                    --dry-run is given: an unprovisioned token fails, it does
#                    not silently skip the proposal.
#
# EXIT  0 proposed (or printed, with --dry-run); 2 cannot propose.
set -euo pipefail
IS_PROGRAM=publish-kyq-drop.sh
here="$(cd -- "${0%/*}" && pwd)"
# shellcheck source=scripts/standards/lib.sh
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# ENVIRONMENT/p' "$0" | sed 's/^# \{0,1\}//'
}

root="$(ijson_standards_root)"
kyq_repo="metadatastician/kyq"
dry_run=0
tmpdir=""
cleanup() {
  if [ -n "$tmpdir" ] && [ -d "$tmpdir" ]; then
    rm -rf -- "$tmpdir"
  fi
}

while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="${2:?--root needs a directory}"; shift 2 ;;
    --kyq-repo) kyq_repo="${2:?--kyq-repo needs OWNER/NAME}"; shift 2 ;;
    --dry-run) dry_run=1; shift ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    *) ijson_die "$IS_EXIT_CANNOT" "unknown option $1" ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"

root="$(cd -- "$root" && pwd)"
drop="$root/interop/kyq"
[ -f "$drop/CONTRACT.adoc" ] || ijson_die "$IS_EXIT_CANNOT" "no contract drop at $drop"
[ -f "$drop/vectors.sha256" ] || ijson_die "$IS_EXIT_CANNOT" "no vectors manifest at $drop"
if ! (cd "$drop" && sha256sum -c --quiet vectors.sha256); then
  ijson_die "$IS_EXIT_WRONG_INPUT" "the vectors do not match $drop/vectors.sha256; re-record them first"
fi

revision="unknown"
if command -v git >/dev/null 2>&1; then
  candidate="$(git -C "$root" rev-parse HEAD 2>/dev/null || true)"
  if [ -n "$candidate" ]; then
    revision="$candidate"
  fi
fi
standards_rev="$(ijson_pin "$root" STANDARDS_REV)"
vectors_sha="$(ijson_sha256 "$drop/vectors.sha256")"
short="${revision:0:12}"
branch="ijson-jcs/contract-$short"

if [ "$dry_run" -eq 1 ]; then
  printf 'kyq-drop: dry run\n  repository: %s\n  branch: %s\n  ijson-jcs revision: %s\n  standards revision: %s\n  vectors manifest: %s\n' \
    "$kyq_repo" "$branch" "$revision" "$standards_rev" "$vectors_sha"
  exit "$IS_EXIT_OK"
fi

[ -n "${KYQ_SYNC_TOKEN:-}" ] ||
  ijson_die "$IS_EXIT_CANNOT" \
    "KYQ_SYNC_TOKEN is not provisioned; the estate mechanism must supply a token that may open pull requests in $kyq_repo"
ijson_require gh "proposing the drop needs the GitHub CLI"
ijson_require git "proposing the drop needs git"

tmpdir="$(mktemp -d)"
trap cleanup EXIT
export GH_TOKEN="$KYQ_SYNC_TOKEN"
# gh's credential helper reads GH_TOKEN from the environment, so the clone and
# the push authenticate without writing the token to disk.
gh auth setup-git >/dev/null 2>&1 ||
  printf 'kyq-drop: gh could not install a credential helper; relying on the environment\n' >&2

gh repo clone "$kyq_repo" "$tmpdir/kyq" -- --depth 1 >/dev/null 2>&1 ||
  ijson_die "$IS_EXIT_CANNOT" "cannot clone $kyq_repo"
target="$tmpdir/kyq/interop/ijson-jcs"
mkdir -p -- "$target"
cp -a -- "$drop/." "$target/"
printf '%s\n' "$revision" >"$target/SOURCE"
printf '%s\n' "$standards_rev" >"$target/STANDARDS-REVISION"

git -C "$tmpdir/kyq" checkout -B "$branch" >/dev/null 2>&1 ||
  ijson_die "$IS_EXIT_CANNOT" "cannot create branch $branch in the clone"
git -C "$tmpdir/kyq" add -- interop/ijson-jcs >/dev/null
staged="$(git -C "$tmpdir/kyq" diff --cached --name-only)"
[ -n "$staged" ] || ijson_die "$IS_EXIT_FINDING" "kyq already carries this drop; nothing to propose"

body="$tmpdir/body"
{
  printf 'Contract drop from `hyperpolymath/ijson-jcs`.\n\n'
  printf '* ijson-jcs revision: `%s`\n' "$revision"
  printf '* standards revision pinned there: `%s`\n' "$standards_rev"
  printf '* vectors manifest sha256: `%s`\n' "$vectors_sha"
  printf '* vectors: %s case(s)\n\n' "$(ls -1 "$drop/vectors" | wc -l | tr -d ' ')"
  printf 'Verifying these vectors is one input to kyq readiness, not a verdict: the\n'
  printf 'stub validator in `src/Kyaml/Ijson.affine` must still be replaced, and\n'
  printf "kyq's own suite and estate gates must pass.\n"
} >"$body"

git -C "$tmpdir/kyq" -c user.name="hyperpolymath-standards-bot" \
  -c user.email="standards-bot@users.noreply.github.com" \
  commit -m "interop: ijson-jcs contract drop at $short" >/dev/null ||
  ijson_die "$IS_EXIT_CANNOT" "cannot commit the drop"
git -C "$tmpdir/kyq" push --force-with-lease origin "$branch" >/dev/null 2>&1 ||
  ijson_die "$IS_EXIT_CANNOT" "cannot push $branch to $kyq_repo"

existing="$(gh pr list --repo "$kyq_repo" --head "$branch" --state open --json number \
  --jq 'if length == 0 then "" else (.[0].number | tostring) end')"
if [ -n "$existing" ]; then
  gh pr edit "$existing" --repo "$kyq_repo" --body-file "$body" >/dev/null ||
    ijson_die "$IS_EXIT_CANNOT" "cannot update pull request $existing"
  printf 'kyq-drop: updated pull request %s in %s\n' "$existing" "$kyq_repo"
else
  url="$(gh pr create --repo "$kyq_repo" --head "$branch" \
    --title "interop: ijson-jcs contract drop at $short" \
    --body-file "$body")" ||
    ijson_die "$IS_EXIT_CANNOT" "cannot open a pull request in $kyq_repo"
  printf 'kyq-drop: opened %s\n' "$url"
fi
