#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# kyq-vectors.sh — verify (or regenerate) the differential vectors kyq's
# AffineScript port is measured against.
#
# WHY: kyq inherits this CLI's contract (docs/IJSON_JCS_INTEGRATION.adoc in
# metadatastician/kyq) and its own validator is still a stub that returns
# success unconditionally. A stub is not a gate, so the contract is pinned by
# vectors that a real binary has to reproduce: the same bytes in, the same
# bytes out, the same exit code. Vectors are bytes, not prose, and their
# manifest is checked first so that nobody can quietly re-baseline a vector to
# match a regression.
#
# USAGE
#   kyq-vectors.sh verify|write [--root DIR] [--binary PATH] [--dir DIR]
#
#   verify   run every vector against --binary and compare with the recorded
#            expectations; also check the vectors match vectors.sha256
#   write    re-record expectations and the manifest from --binary (a proposal,
#            to be reviewed like any other change to the contract)
#
#   --root DIR     repository root (default: this checkout)
#   --binary PATH  candidate binary (default: target/release/ijson-jcs)
#   --dir DIR      interop/kyq directory (default: <root>/interop/kyq)
#
# VECTOR LAYOUT (directories under <dir>/vectors, one per case)
#   cmd                  the subcommand and flags, no file argument
#   stdin                bytes written to the file the binary is given
#   filename             optional file name (default case.json)
#   expect.exit          the exit code the binary must produce
#   expect.stdout        optional exact stdout bytes
#   expect.stdout_prefix optional stdout prefix (for messages carrying a reason)
#   expect.stderr_contains optional substring the stderr must contain
#   expect.file          optional exact file contents after the run
#
# EXIT  0 every vector matched and the manifest holds; 1 a mismatch (named);
#       2 cannot run (no vectors, no binary, missing tool).
set -euo pipefail
IS_PROGRAM=kyq-vectors.sh
# shellcheck source=scripts/standards/lib.sh
here="$(cd -- "${0%/*}" && pwd)"
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# EXIT/p' "$0" | sed 's/^# \{0,1\}//'
}

mode=""
root="$(ijson_standards_root)"
binary=""
dir=""
tmpdir=""

cleanup() {
  if [ -n "$tmpdir" ] && [ -d "$tmpdir" ]; then
    rm -rf -- "$tmpdir"
  fi
}

while [ $# -gt 0 ]; do
  case "$1" in
    verify | write) mode="$1"; shift ;;
    --root) root="${2:?--root needs a directory}"; shift 2 ;;
    --binary) binary="${2:?--binary needs a path}"; shift 2 ;;
    --dir) dir="${2:?--dir needs a directory}"; shift 2 ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    *) ijson_die "$IS_EXIT_CANNOT" "unknown argument $1" ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"
case "$mode" in
  verify | write) : ;;
  '') ijson_die "$IS_EXIT_CANNOT" "pass verify or write" ;;
  *) ijson_die "$IS_EXIT_CANNOT" "unknown mode: $mode" ;;
esac

root="$(cd -- "$root" && pwd)"
[ -n "$dir" ] || dir="$root/interop/kyq"
[ -d "$dir/vectors" ] || ijson_die "$IS_EXIT_CANNOT" "no vectors at $dir/vectors"
[ -n "$binary" ] || binary="$root/target/release/ijson-jcs"
[ -x "$binary" ] || ijson_die "$IS_EXIT_CANNOT" "no executable binary at $binary"
manifest="$dir/vectors.sha256"

# check_manifest — the recorded bytes are the reviewed bytes.
check_manifest() {
  [ -f "$manifest" ] || ijson_die "$IS_EXIT_CANNOT" "no manifest at $manifest"
  local status=0
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$dir" && sha256sum -c --quiet vectors.sha256) || status=$?
  else
    (cd "$dir" && shasum -a 256 -c vectors.sha256) || status=$?
  fi
  if [ "$status" -ne 0 ]; then
    printf 'the vectors do not match %s; review the edit, then re-record with: %s write\n' \
      "$manifest" "$0" >&2
    return "$IS_EXIT_FINDING"
  fi
}

# write_manifest — sha256 of every vector file, in a stable order.
write_manifest() {
  (
    cd "$dir"
    find vectors -type f -print0 | sort -z | xargs -0 sha256sum
  ) >"$manifest"
}

# clip FILE [MAX] — one printable line of a file's bytes, for a FAIL line.
clip() {
  local file="$1" max="${2:-160}" text
  if [ -f "$file" ]; then
    text="$(od -An -v -c -- "$file" | tr -s ' ' | tr '\n' ' ' | sed 's/^ //; s/ $//')"
    printf '%s' "${text:0:$max}"
  else
    printf '(missing)'
  fi
}

# show FILE — a bounded, printable rendering for a mismatch report.
show() {
  if [ -f "$1" ]; then
    od -An -c -- "$1" | head -n 8
  else
    printf '(missing)\n'
  fi
}

if [ "$mode" = verify ]; then
  check_manifest || exit "$IS_EXIT_FINDING"
fi

tmpdir="$(mktemp -d)"
trap cleanup EXIT

passed=0
failed=0
for case_dir in "$dir"/vectors/*/; do
  id="$(basename -- "$case_dir")"
  [ -f "$case_dir/cmd" ] || ijson_die "$IS_EXIT_WRONG_INPUT" "$id has no cmd file"
  [ -f "$case_dir/stdin" ] || ijson_die "$IS_EXIT_WRONG_INPUT" "$id has no stdin file"
  mapfile -t argv <"$case_dir/cmd"
  [ "${#argv[@]}" -gt 0 ] || ijson_die "$IS_EXIT_WRONG_INPUT" "$id has an empty cmd file"
  filename=case.json
  if [ -f "$case_dir/filename" ]; then
    filename="$(head -n 1 "$case_dir/filename")"
  fi

  work="$tmpdir/$id"
  mkdir -p "$work"
  cp -- "$case_dir/stdin" "$work/$filename"

  got_exit=0
  case " ${argv[*]} " in
    *" canon "* | *" jsonld "*)
      (cd "$work" && "$binary" "${argv[@]}" <"$work/$filename") >"$tmpdir/stdout" 2>"$tmpdir/stderr" ||
        got_exit=$?
      ;;
    *)
      (cd "$work" && "$binary" "${argv[@]}" -- "$filename") >"$tmpdir/stdout" 2>"$tmpdir/stderr" ||
        got_exit=$?
      ;;
  esac

  if [ "$mode" = write ]; then
    # Re-recording may not weaken a vector. The prefixes and substrings are the
    # curated part of the contract (a refusal has to keep saying why), so a new
    # behaviour that no longer satisfies them is a finding for review, not a
    # licence to delete the assertion.
    curated=0
    if [ -f "$case_dir/expect.stdout_prefix" ]; then
      prefix="$(head -n 1 "$case_dir/expect.stdout_prefix")"
      if [ "$(head -n 1 "$tmpdir/stdout")" != "$prefix" ]; then
        printf 'REFUSED %s: stdout no longer starts with %s\n' "$id" "$prefix" >&2
        show "$tmpdir/stdout" >&2
        curated=1
      fi
    fi
    if [ -f "$case_dir/expect.stderr_contains" ]; then
      needle="$(head -n 1 "$case_dir/expect.stderr_contains")"
      if ! grep -qF -- "$needle" "$tmpdir/stderr"; then
        printf 'REFUSED %s: stderr no longer contains %s\n' "$id" "$needle" >&2
        curated=1
      fi
    fi
    if [ "$curated" -ne 0 ]; then
      failed=$((failed + 1))
      continue
    fi
    printf '%s\n' "$got_exit" >"$case_dir/expect.exit"
    if [ -s "$tmpdir/stdout" ]; then
      cp -- "$tmpdir/stdout" "$case_dir/expect.stdout"
    else
      rm -f -- "$case_dir/expect.stdout"
    fi
    rm -f -- "$case_dir/expect.stderr"
    if [ -f "$work/$filename" ] && ! cmp -s -- "$work/$filename" "$case_dir/stdin"; then
      cp -- "$work/$filename" "$case_dir/expect.file"
    else
      rm -f -- "$case_dir/expect.file"
    fi
    printf 'recorded %s\n' "$id"
    passed=$((passed + 1))
    continue
  fi

  case_failed=0
  reasons=""
  want_exit="$(head -n 1 "$case_dir/expect.exit" 2>/dev/null || true)"
  if [ -z "$want_exit" ]; then
    reasons+="no expect.exit; "
    case_failed=1
  elif [ "$got_exit" != "$want_exit" ]; then
    reasons+="exit $got_exit want $want_exit; "
    case_failed=1
  fi
  if [ -f "$case_dir/expect.stdout" ] && ! cmp -s -- "$tmpdir/stdout" "$case_dir/expect.stdout"; then
    reasons+="stdout=[$(clip "$tmpdir/stdout")] want=[$(clip "$case_dir/expect.stdout")]; "
    case_failed=1
  fi
  if [ -f "$case_dir/expect.stdout_prefix" ]; then
    prefix="$(head -n 1 "$case_dir/expect.stdout_prefix")"
    if [ "$(head -n 1 "$tmpdir/stdout")" != "$prefix" ]; then
      reasons+="stdout prefix is not [$prefix] but [$(clip "$tmpdir/stdout")]; "
      case_failed=1
    fi
  fi
  if [ -f "$case_dir/expect.stderr_contains" ]; then
    needle="$(head -n 1 "$case_dir/expect.stderr_contains")"
    if ! grep -qF -- "$needle" "$tmpdir/stderr"; then
      reasons+="stderr lacks [$needle], has [$(clip "$tmpdir/stderr")]; "
      case_failed=1
    fi
  fi
  if [ -f "$case_dir/expect.file" ] && ! cmp -s -- "$work/$filename" "$case_dir/expect.file"; then
    reasons+="file=[$(clip "$work/$filename")] want=[$(clip "$case_dir/expect.file")]; "
    case_failed=1
  fi
  if [ "$case_failed" -ne 0 ]; then
    printf 'FAIL %s: %s\n' "$id" "${reasons%; }" >&2
  fi

  if [ "$case_failed" -eq 0 ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
  fi
done

if [ "$mode" = write ]; then
  if [ "$failed" -ne 0 ]; then
    printf 'kyq-vectors: %d recorded, %d refused; the manifest was not rewritten\n' \
      "$passed" "$failed" >&2
    exit "$IS_EXIT_FINDING"
  fi
  write_manifest
  printf 'kyq-vectors: recorded %d vector(s) and %s\n' "$passed" "$manifest"
  exit "$IS_EXIT_OK"
fi

if [ "$failed" -ne 0 ]; then
  printf 'kyq-vectors: %d passed, %d failed\n' "$passed" "$failed" >&2
  exit "$IS_EXIT_FINDING"
fi
printf 'kyq-vectors: %d vector(s) match the candidate binary\n' "$passed"
