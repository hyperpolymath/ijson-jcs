#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# fetch.sh — materialise one immutable standards revision and check it against
# the recorded manifest, or record a manifest for a candidate revision.
#
# WHY: the pin in .ci/pins.sh is a commit, and .ci/standards.sha256 says what
# each pinned file must hash to. A checkout of `main` is not a pin, and a file
# that "looks right" is not evidence. So every run re-derives the bytes from the
# commit and compares them with the record; a mismatch is exit 3, an unreachable
# revision is exit 2, and neither is a pass.
#
# Nothing here runs from a moving ref at run time: the revision is always a full
# 40-hex commit from the pin file, from --rev, or from a proposal that a human
# reviews.
#
# USAGE
#   fetch.sh [--root DIR] [--rev SHA] [--dest DIR] [--source DIR]
#            [--manifest FILE] [--manifest-out FILE] [--no-download]
#
#   --root DIR          repository root holding .ci/ (default: this checkout)
#   --rev SHA           revision to fetch (default: STANDARDS_REV from pins)
#   --dest DIR          checkout destination (default: target/standards/<rev>)
#   --source DIR        already-materialised tree to verify instead of fetching
#   --manifest FILE     manifest to verify against (default: .ci/standards.sha256)
#   --manifest-out FILE record hashes for --rev into FILE instead of verifying
#   --no-download       never use the network; require --source or a cache hit
#
# OUTPUT
#   stdout: the resolved tree path, and nothing else, so callers can capture it.
#   stderr: progress and every diagnosis.
#
# EXIT  0 verified or recorded; 2 cannot run (tool, network, usage); 3 the
#       pinned inputs are wrong (missing file, hash mismatch, unpinnable path).
set -euo pipefail
IS_PROGRAM=fetch.sh
# shellcheck source=scripts/standards/lib.sh
here="$(cd -- "${0%/*}" && pwd)"
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# EXIT/p' "$0" | sed 's/^# \{0,1\}//'
}

root="$(ijson_standards_root)"
rev=""
dest=""
source_dir=""
manifest=""
manifest_out=""
allow_download=1
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
    --dest) dest="${2:?--dest needs a directory}"; shift 2 ;;
    --source) source_dir="${2:?--source needs a directory}"; shift 2 ;;
    --manifest) manifest="${2:?--manifest needs a file}"; shift 2 ;;
    --manifest-out) manifest_out="${2:?--manifest-out needs a file}"; shift 2 ;;
    --no-download) allow_download=0; shift ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    *) ijson_die "$IS_EXIT_CANNOT" "unknown option $1" ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"

root="$(cd -- "$root" && pwd)"
[ -n "$rev" ] || rev="$(ijson_pin "$root" STANDARDS_REV)"
ijson_is_revision "$rev" || ijson_die "$IS_EXIT_CANNOT" "--rev must be a full 40-hex commit, got '$rev'"
[ -n "$manifest" ] || manifest="$root/.ci/standards.sha256"
[ -f "$manifest" ] || ijson_die "$IS_EXIT_CANNOT" "manifest not found: $manifest"
ijson_validate_manifest "$manifest"
contract="$root/.ci/standards-contract.tsv"
[ -f "$contract" ] || ijson_die "$IS_EXIT_CANNOT" "contract not found: $contract"
ijson_validate_contract "$contract"

# download URL TARBALL — fetch the revision archive, or exit 2.
download() {
  local url="$1"
  local out="$2"
  local partial="$out.part.$$"
  mkdir -p -- "$(dirname -- "$out")"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 2 --max-time 180 -o "$partial" "$url" ||
      { rm -f -- "$partial"; ijson_die "$IS_EXIT_CANNOT" "cannot download $url"; }
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$partial" "$url" ||
      { rm -f -- "$partial"; ijson_die "$IS_EXIT_CANNOT" "cannot download $url"; }
  else
    ijson_die "$IS_EXIT_CANNOT" "neither curl nor wget is on PATH; use --source or a cached tree"
  fi
  mv -- "$partial" "$out"
}

# materialise — the tree to inspect, downloading only when there is no cache hit.
materialise() {
  if [ -n "$source_dir" ]; then
    [ -d "$source_dir" ] || ijson_die "$IS_EXIT_CANNOT" "no such source tree: $source_dir"
    (cd -- "$source_dir" && pwd)
    return
  fi
  local cache_root="$root/target/standards"
  local target="${dest:-$cache_root/$rev}"
  if [ -f "$target/.standards-rev" ] && [ "$(cat -- "$target/.standards-rev")" = "$rev" ]; then
    printf 'fetch: using cached tree %s\n' "$target" >&2
    printf '%s\n' "$target"
    return
  fi
  [ "$allow_download" -eq 1 ] || ijson_die "$IS_EXIT_CANNOT" "no cached tree for $rev and --no-download was given"
  local base
  base="$(ijson_pin "$root" STANDARDS_URL_BASE)"
  local slug
  slug="$(ijson_pin "$root" STANDARDS_REPOSITORY)"
  local tarball="$root/target/standards-cache/$rev.tar.gz"
  if [ ! -f "$tarball" ]; then
    download "${base%/}/$slug/tar.gz/$rev" "$tarball"
  else
    printf 'fetch: using cached archive %s\n' "$tarball" >&2
  fi
  tmpdir="$(mktemp -d)"
  trap cleanup EXIT
  tar -xzf "$tarball" -C "$tmpdir" || ijson_die "$IS_EXIT_CANNOT" "cannot extract $tarball"
  local extracted
  extracted="$(find "$tmpdir" -maxdepth 1 -mindepth 1 -type d | head -n 1)"
  [ -n "$extracted" ] || ijson_die "$IS_EXIT_CANNOT" "archive $tarball has no top-level directory"
  mkdir -p -- "$(dirname -- "$target")"
  rm -rf -- "$target"
  mv -- "$extracted" "$target"
  printf '%s\n' "$rev" >"$target/.standards-rev"
  printf '%s\n' "$target"
}

# verify TREE — every pinned path matches its recorded digest, and every
# contract path is pinned at all.
verify() {
  local tree="$1"
  local count=0 digest path actual
  while read -r digest path; do
    if [ ! -f "$tree/$path" ]; then
      ijson_die "$IS_EXIT_WRONG_INPUT" "pinned file missing at $rev: $path"
    fi
    actual="$(ijson_sha256 "$tree/$path")"
    if [ "$actual" != "$digest" ]; then
      ijson_die "$IS_EXIT_WRONG_INPUT" \
        "pinned file changed at $rev: $path (recorded $digest, found $actual)"
    fi
    count=$((count + 1))
  done < <(ijson_manifest_entries "$manifest")
  [ "$count" -gt 0 ] || ijson_die "$IS_EXIT_WRONG_INPUT" "manifest $manifest pins no files"

  local contract_path
  while read -r contract_path; do
    if ! grep -qxF -- "$contract_path" <(ijson_manifest_entries "$manifest" | awk '{print $2}'); then
      ijson_die "$IS_EXIT_WRONG_INPUT" \
        "contract path is not pinned in $manifest: $contract_path"
    fi
  done < <(ijson_contract_paths "$contract")

  printf 'fetch: verified %d pinned file(s) at %s\n' "$count" "$rev" >&2
}

# record TREE OUT — the manifest a proposal would add for this revision.
#
# The path list is the union of what is pinned today and what the contract
# names, so a proposal can never quietly shrink the record. A path that no
# longer exists at the candidate revision is exit 3: dropping a pinned input is
# a policy decision for a human, not something a bot may infer.
record() {
  local tree="$1"
  local out="$2"
  [ -n "$tmpdir" ] || tmpdir="$(mktemp -d)"
  trap cleanup EXIT
  local paths="$tmpdir/paths"
  : >"$paths"
  ijson_manifest_entries "$manifest" | awk '{print $2}' >>"$paths"
  ijson_contract_paths "$contract" >>"$paths"
  sort -u -o "$paths" "$paths"
  [ -s "$paths" ] || ijson_die "$IS_EXIT_WRONG_INPUT" "nothing to pin: manifest and contract are empty"

  local generated="$tmpdir/manifest"
  local missing=0 path
  : >"$generated"
  while read -r path; do
    if [ ! -f "$tree/$path" ]; then
      printf 'pinned input missing at %s: %s\n' "$rev" "$path" >&2
      missing=1
      continue
    fi
    printf '%s  %s\n' "$(ijson_sha256 "$tree/$path")" "$path" >>"$generated"
  done <"$paths"
  if [ "$missing" -ne 0 ]; then
    ijson_die "$IS_EXIT_WRONG_INPUT" \
      "the candidate revision no longer carries a pinned input; review the manifest change by hand"
  fi
  mkdir -p -- "$(dirname -- "$out")"
  mv -- "$generated" "$out"
  printf 'fetch: recorded %d pinned file(s) for %s in %s\n' \
    "$(wc -l <"$out" | tr -d ' ')" "$rev" "$out" >&2
}

tree="$(materialise)"
if [ -n "$manifest_out" ]; then
  record "$tree" "$manifest_out"
else
  verify "$tree"
fi
printf '%s\n' "$tree"
