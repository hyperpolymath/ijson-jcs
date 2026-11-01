#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# propose.sh — turn a standards revision into a reviewable change to this
# repository's pin, and nothing else.
#
# WHY: the pin is evidence. Moving it is a reviewed act, so this script only
# prepares the change: it
#
#   1. refuses anything that is not a full 40-hex commit;
#   2. refuses a revision that drops a pinned input (a proposal may add, never
#      silently remove: dropping a normative input needs a human ruling);
#   3. refuses a revision whose policy no longer says what the contract says
#      (contract.sh, exit 3);
#   4. writes .ci/pins.sh, .ci/standards.sha256 and appends to
#      .ci/standards-revisions.tsv — nothing else, and it commits nothing.
#
# The CI workflow that calls this then runs verify.sh against the *candidate*
# binary before it opens a pull request, and the pull request carries the
# receipt. A human merges it. There is no path here that upgrades the pin
# without a verified binary and a review.
#
# USAGE
#   propose.sh [--root DIR] [--rev SHA] [--head SHA] [--source DIR] [--dry-run]
#
#   --root DIR    repository root (default: this checkout)
#   --rev SHA     candidate revision (default: the head detect.sh reports)
#   --head SHA    upstream head to compare against, without the network
#   --source DIR  already-materialised candidate tree (tests, offline review)
#   --dry-run     report the change and write nothing
#
# EXIT  0 a proposal was written (or would be, with --dry-run); 1 already
#       pinned, nothing to do; 2 cannot run; 3 the candidate revision is
#       incompatible (pinned input missing, contract no longer holds).
set -euo pipefail
IS_PROGRAM=propose.sh
# shellcheck source=scripts/standards/lib.sh
here="$(cd -- "${0%/*}" && pwd)"
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# EXIT/p' "$0" | sed 's/^# \{0,1\}//'
}

root="$(ijson_standards_root)"
rev=""
head=""
source_dir=""
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
    --rev) rev="${2:?--rev needs a commit}"; shift 2 ;;
    --head) head="${2:?--head needs a commit}"; shift 2 ;;
    --source) source_dir="${2:?--source needs a directory}"; shift 2 ;;
    --dry-run) dry_run=1; shift ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    *) ijson_die "$IS_EXIT_CANNOT" "unknown option $1" ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"

root="$(cd -- "$root" && pwd)"
scripts_dir="$here"
pin="$(ijson_pin "$root" STANDARDS_REV)"
ijson_is_revision "$pin" || ijson_die "$IS_EXIT_CANNOT" "pinned revision is not a full commit: $pin"

if [ -z "$rev" ]; then
  detect_args=(--root "$root")
  if [ -n "$head" ]; then
    detect_args+=(--head "$head")
  fi
  # detect.sh exits 1 on drift; that is the case we are here for.
  drift_report="$(bash "$scripts_dir/detect.sh" "${detect_args[@]}" 2>/dev/null)" || true
  rev="$(printf '%s\n' "$drift_report" | sed -n 's/^head=//p')"
fi
ijson_is_revision "$rev" || ijson_die "$IS_EXIT_CANNOT" "candidate revision is not a full commit: '$rev'"

if [ "$rev" = "$pin" ]; then
  printf 'propose: already pinned at %s; nothing to do\n' "$pin"
  exit "$IS_EXIT_FINDING"
fi

tmpdir="$(mktemp -d)"
trap cleanup EXIT

# 1. Materialise the candidate and record what it would pin. fetch.sh fails
#    closed (exit 3) when a pinned input has disappeared at the revision.
fetch_args=(--root "$root" --rev "$rev" --manifest-out "$tmpdir/standards.sha256")
if [ -n "$source_dir" ]; then
  fetch_args+=(--source "$source_dir")
fi
tree="$(bash "$scripts_dir/fetch.sh" "${fetch_args[@]}")" ||
  { ijson_die "$IS_EXIT_WRONG_INPUT" "cannot record a manifest for $rev"; }

# 2. The policies it pins must still say what the tests assert.
bash "$scripts_dir/contract.sh" --root "$root" --tree "$tree" --rev "$rev" ||
  { ijson_die "$IS_EXIT_WRONG_INPUT" "$rev is not compatible with the recorded contract"; }

# 3. Report exactly which recorded hashes move, for the reviewer and the PR body.
changed=0
added=0
while read -r digest path; do
  old="$(awk -v want="$path" '$2 == want { print $1; exit }' "$root/.ci/standards.sha256")"
  if [ -z "$old" ]; then
    printf 'ADDED   %s\n' "$path"
    added=$((added + 1))
  elif [ "$old" != "$digest" ]; then
    printf 'CHANGED %s\n' "$path"
    changed=$((changed + 1))
  fi
done < <(ijson_manifest_entries "$tmpdir/standards.sha256")
total="$(wc -l <"$tmpdir/standards.sha256" | tr -d ' ')"
printf 'propose: standards %s -> %s: %s file(s) pinned, %d changed, %d added\n' \
  "${pin:0:12}" "${rev:0:12}" "$total" "$changed" "$added"

if [ "$dry_run" -eq 1 ]; then
  printf 'propose: --dry-run, nothing written\n'
  exit "$IS_EXIT_OK"
fi

# 4. Write the pin, the manifest and the append-only revision log.
pins="$root/.ci/pins.sh"
[ -f "$pins" ] || ijson_die "$IS_EXIT_CANNOT" "pin file not found: $pins"
[ "$(grep -c "^readonly STANDARDS_REV=" "$pins")" = "1" ] ||
  ijson_die "$IS_EXIT_WRONG_INPUT" "$pins must declare STANDARDS_REV exactly once"
sed "s/^readonly STANDARDS_REV='[0-9a-f]\\{40\\}'$/readonly STANDARDS_REV='$rev'/" \
  "$pins" >"$tmpdir/pins.sh"
if ! grep -qxF "readonly STANDARDS_REV='$rev'" "$tmpdir/pins.sh"; then
  ijson_die "$IS_EXIT_WRONG_INPUT" "could not rewrite STANDARDS_REV in $pins"
fi
mv -- "$tmpdir/pins.sh" "$pins"

cp -- "$tmpdir/standards.sha256" "$root/.ci/standards.sha256"
manifest_sha="$(ijson_sha256 "$root/.ci/standards.sha256")"

revisions="$root/.ci/standards-revisions.tsv"
if [ ! -f "$revisions" ] || [ ! -s "$revisions" ]; then
  printf 'date\trevision\tprevious\tmanifest_sha256\tsource\n' >"$revisions"
fi
source_commit="unknown"
if command -v git >/dev/null 2>&1; then
  candidate="$(git -C "$root" rev-parse HEAD 2>/dev/null || true)"
  if [ -n "$candidate" ]; then
    source_commit="$candidate"
  fi
fi
printf '%s\t%s\t%s\t%s\t%s\n' \
  "$(ijson_utc_now)" "$rev" "$pin" "$manifest_sha" "$source_commit" >>"$revisions"

printf 'propose: wrote .ci/pins.sh, .ci/standards.sha256 (%s) and .ci/standards-revisions.tsv\n' \
  "${manifest_sha:0:12}"
