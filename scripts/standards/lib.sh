#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# Shared helpers for the standards update pipeline. Source this file; running
# it does nothing.
#
# One exit contract, used by every script in scripts/standards/:
#
#   0  the check ran and found nothing
#   1  the check ran and found something: drift, a finding, a proposal
#   2  the check could not run: missing tool, missing input, bad usage,
#      unreachable remote. Never a pass.
#   3  the pinned inputs are wrong: a manifest hash mismatch, a pinned file
#      missing at its revision, or an incompatible policy change. Never a pass.
#
# The two failure codes are deliberately distinct: 2 means "we could not look",
# 3 means "we looked and what we found is not what was recorded". Collapsing
# them (or returning 0) is the defect this pipeline exists to prevent.

# shellcheck shell=bash

readonly IS_EXIT_OK=0
readonly IS_EXIT_FINDING=1
readonly IS_EXIT_CANNOT=2
readonly IS_EXIT_WRONG_INPUT=3

# ijson_standards_root — the repository root of the calling script.
ijson_standards_root() {
  local source_dir
  source_dir="$(cd -- "${BASH_SOURCE[1]%/*}" && pwd)"
  (cd -- "$source_dir/../.." && pwd)
}

# ijson_die STATUS MESSAGE... — report on stderr and exit with STATUS.
ijson_die() {
  local status="$1"
  shift
  printf '%s: %s\n' "${IS_PROGRAM:-standards}" "$*" >&2
  exit "$status"
}

# ijson_require COMMAND [HINT] — a missing tool is exit 2, never a skip.
ijson_require() {
  local command_name="$1"
  local hint="${2:-}"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    if [ -n "$hint" ]; then
      ijson_die "$IS_EXIT_CANNOT" "$command_name is not on PATH; $hint"
    fi
    ijson_die "$IS_EXIT_CANNOT" "$command_name is not on PATH"
  fi
}

# ijson_require_hash_tool — sha256sum, or shasum on a system without coreutils.
# Sets IS_HASH_TOOL to the command line prefix to use.
ijson_require_hash_tool() {
  if command -v sha256sum >/dev/null 2>&1; then
    IS_HASH_TOOL=(sha256sum)
  elif command -v shasum >/dev/null 2>&1; then
    IS_HASH_TOOL=(shasum -a 256)
  else
    ijson_die "$IS_EXIT_CANNOT" "neither sha256sum nor shasum is on PATH"
  fi
}

# ijson_sha256 FILE — the lowercase hex digest of FILE.
ijson_sha256() {
  ijson_require_hash_tool
  "${IS_HASH_TOOL[@]}" -- "$1" | cut -d' ' -f1
}

# ijson_pin ROOT NAME — the value of a variable in ROOT/.ci/pins.sh.
ijson_pin() {
  local root="$1"
  local name="$2"
  (
    # shellcheck disable=SC1091
    . "$root/.ci/pins.sh"
    printf '%s\n' "${!name}"
  )
}

# ijson_is_revision VALUE — a full 40-hex commit, and nothing else.
ijson_is_revision() {
  [[ "$1" =~ ^[0-9a-f]{40}$ ]]
}

# ijson_utc_now — an ISO 8601 UTC timestamp.
ijson_utc_now() {
  ijson_require date
  date -u +%Y-%m-%dT%H:%M:%SZ
}

# ijson_validate_manifest FILE — refuse a manifest line we cannot read.
#
# This reads the file in the *current* shell, so ijson_die stops the script. The
# same checks inside ijson_manifest_entries run in a process substitution, whose
# `exit` would only end that subshell: callers validate first, then iterate.
ijson_validate_manifest() {
  local file="$1"
  local line
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      '' | '#'*) continue ;;
    esac
    if [[ ! "$line" =~ ^([0-9a-f]{64})\ \ (.+)$ ]]; then
      ijson_die "$IS_EXIT_WRONG_INPUT" "malformed manifest line in $file: $line"
    fi
    case "${BASH_REMATCH[2]}" in
      /* | ../* | */../* | */..) ijson_die "$IS_EXIT_WRONG_INPUT" "manifest path escapes the tree: ${BASH_REMATCH[2]}" ;;
    esac
  done <"$file"
}

# ijson_validate_contract FILE — refuse a contract line that is not three fields.
ijson_validate_contract() {
  local file="$1"
  local line
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      '' | '#'*) continue ;;
    esac
    if [ "$(printf '%s' "$line" | awk -F'\t' '{print NF}')" != "3" ]; then
      ijson_die "$IS_EXIT_WRONG_INPUT" "malformed contract line in $file: $line"
    fi
  done <"$file"
}

# ijson_manifest_entries FILE — print each `<sha256>  <path>` as two fields,
# skipping comments and blank lines. Refuses a malformed line: a manifest with
# a line we cannot read is exit 3, not a line we quietly ignore.
ijson_manifest_entries() {
  local file="$1"
  local line digest path
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      '' | '#'*) continue ;;
    esac
    if [[ ! "$line" =~ ^([0-9a-f]{64})\ \ (.+)$ ]]; then
      ijson_die "$IS_EXIT_WRONG_INPUT" "malformed manifest line in $file: $line"
    fi
    digest="${BASH_REMATCH[1]}"
    path="${BASH_REMATCH[2]}"
    case "$path" in
      /* | ../* | */../* | */..) ijson_die "$IS_EXIT_WRONG_INPUT" "manifest path escapes the tree: $path" ;;
    esac
    printf '%s %s\n' "$digest" "$path"
  done <"$file"
}

# ijson_contract_entries FILE — print each `<role>\t<path>\t<literal>` record.
# A line that is not exactly three TAB-separated fields is exit 3.
ijson_contract_entries() {
  local file="$1"
  local line
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      '' | '#'*) continue ;;
    esac
    if [ "$(printf '%s' "$line" | awk -F'\t' '{print NF}')" != "3" ]; then
      ijson_die "$IS_EXIT_WRONG_INPUT" "malformed contract line in $file: $line"
    fi
    printf '%s\n' "$line"
  done <"$file"
}

# ijson_contract_paths FILE — the distinct paths a contract pins, in order.
ijson_contract_paths() {
  ijson_contract_entries "$1" | cut -f2 | awk '!seen[$0]++'
}

# ijson_check_summary PASS FAIL [EXTRA] — one machine-readable last line.
ijson_check_summary() {
  local passed="$1"
  local failed="$2"
  local extra="${3:-}"
  printf 'passed %d, failed %d%s\n' "$passed" "$failed" "$extra"
  [ "$failed" -eq 0 ]
}
