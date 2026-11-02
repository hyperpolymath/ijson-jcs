#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# verify.sh — prove a candidate binary against one pinned standards revision,
# and write the receipt that records the run.
#
# WHY: "runs on main" is not evidence. For one immutable revision this runs,
# in order, and fails on the first thing that cannot be proved:
#
#   1. the pinned inputs hash to what .ci/standards.sha256 records;
#   2. the pinned policies still contain every contracted normative sentence;
#   3. `cargo build --locked` and `cargo test --locked --all-features`;
#   4. the *pinned standards* gate test, with the candidate binary on PATH;
#   5. that same gate over a known-canonical tree, and — the controls that make
#      it a gate — over a planted non-canonical tree (must exit 1), with a
#      missing tool (must exit 2) and with a missing path (must exit 2);
#   6. the library/estate-file profile matrix, JSON Lines framing and JSON-LD
#      document scope, all through the candidate binary;
#   7. the kyq differential vectors, if they are present;
#   8. the receipt itself, which the candidate binary must accept as an estate
#      file (a canonical JSON document with exactly one final LF).
#
# A missing tool, a missing fixture or a failed test is a failure. Nothing here
# is skipped, and no step is allowed to pass because it produced no output.
#
# USAGE
#   verify.sh [--root DIR] [--rev SHA] [--binary PATH] [--no-build]
#             [--receipt FILE] [--source DIR] [--label TAG]
#
#   --root DIR     repository root (default: this checkout)
#   --rev SHA      standards revision to verify against (default: the pin)
#   --binary PATH  binary to test (default: target/release/ijson-jcs)
#   --no-build     do not run cargo build; fail unless --binary exists
#   --receipt FILE write the accepted receipt here (canonical JSON + one LF)
#   --source DIR   already-materialised standards tree (offline review)
#   --label TAG    free-form tag recorded in the receipt
#
# EXIT  0 every step passed and the receipt (if asked for) was written;
#       1 a step failed; 2 a step could not run; 3 the pinned inputs are wrong.
set -euo pipefail
IS_PROGRAM=verify.sh
# shellcheck source=scripts/standards/lib.sh
here="$(cd -- "${0%/*}" && pwd)"
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# EXIT/p' "$0" | sed 's/^# \{0,1\}//'
}

root="$(ijson_standards_root)"
rev=""
binary=""
no_build=0
receipt=""
source_dir=""
label=""
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
    --binary) binary="${2:?--binary needs a path}"; shift 2 ;;
    --no-build) no_build=1; shift ;;
    --receipt) receipt="${2:?--receipt needs a file}"; shift 2 ;;
    --source) source_dir="${2:?--source needs a directory}"; shift 2 ;;
    --label) label="${2:?--label needs a value}"; shift 2 ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    *) ijson_die "$IS_EXIT_CANNOT" "unknown option $1" ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"

root="$(cd -- "$root" && pwd)"
scripts_dir="$here"
tmpdir="$(mktemp -d)"
trap cleanup EXIT

[ -n "$rev" ] || rev="$(ijson_pin "$root" STANDARDS_REV)"
ijson_is_revision "$rev" || ijson_die "$IS_EXIT_CANNOT" "--rev must be a full 40-hex commit, got '$rev'"

passed=0
failed=0
names=()
statuses=()
ok() {
  passed=$((passed + 1))
  names+=("$1")
  statuses+=("pass")
  printf '[ok]   %s\n' "$1"
}
bad() {
  failed=$((failed + 1))
  names+=("$1")
  statuses+=("fail")
  printf '[FAIL] %s: %s\n' "$1" "$2" >&2
}
note() { printf '[note] %s\n' "$*"; }

# expect_exit WANT NAME COMMAND... — a command whose exit code is the assertion.
expect_exit() {
  local want="$1"
  local name="$2"
  shift 2
  local got=0
  "$@" >"$tmpdir/last.out" 2>&1 || got=$?
  if [ "$got" = "$want" ]; then
    ok "$name"
  else
    bad "$name" "wanted exit $want, got $got: $(tail -n 3 "$tmpdir/last.out" | tr '\n' ' ')"
  fi
}

# 1. Pinned inputs. fetch.sh exits 3 on a hash mismatch, which is fatal here.
fetch_args=(--root "$root" --rev "$rev")
if [ -n "$source_dir" ]; then
  fetch_args+=(--source "$source_dir")
fi
tree="$(bash "$scripts_dir/fetch.sh" "${fetch_args[@]}")" ||
  ijson_die "$IS_EXIT_WRONG_INPUT" "the pinned standards inputs do not match .ci/standards.sha256 at $rev"
ok "pinned-standards-inputs"

# 2. Policies still say what the contract asserts.
bash "$scripts_dir/contract.sh" --root "$root" --tree "$tree" --rev "$rev" >"$tmpdir/contract.out" 2>&1 ||
  { cat "$tmpdir/contract.out" >&2; ijson_die "$IS_EXIT_WRONG_INPUT" "the pinned policy contract does not hold at $rev"; }
ok "policy-contract"

# 3. Build and test the candidate.
if [ "$no_build" -eq 0 ]; then
  ijson_require cargo "a candidate binary has to be built; install Rust or pass --binary"
  if (cd "$root" && cargo build --release --locked); then
    ok "cargo-build"
  else
    bad "cargo-build" "cargo build --release --locked failed"
  fi
fi
[ -n "$binary" ] || binary="$root/target/release/ijson-jcs"
if [ ! -x "$binary" ]; then
  ijson_die "$IS_EXIT_CANNOT" "no candidate binary at $binary; build it or pass --binary"
fi
case "$binary" in
  */*) binary_dir="${binary%/*}" ;;
  *) binary_dir=. ;;
esac
binary="$(cd -- "$binary_dir" && pwd)/${binary##*/}"

if [ "$no_build" -eq 0 ]; then
  ijson_require cargo "cargo test needs the toolchain"
  if (cd "$root" && cargo test --locked --all-features); then
    ok "cargo-test"
  else
    bad "cargo-test" "cargo test --locked --all-features failed"
  fi
fi

# 4. The pinned standards gate test, against the candidate binary.
gate_test="$tree/scripts/tests/check-ijson-jcs-test.sh"
[ -f "$gate_test" ] || ijson_die "$IS_EXIT_WRONG_INPUT" "pinned gate test missing: $gate_test"
if IJSON_JCS="$binary" bash "$gate_test" >"$tmpdir/gate-test.out" 2>&1; then
  summary="$(tail -n 1 "$tmpdir/gate-test.out")"
  if printf '%s\n' "$summary" | grep -Eq '^passed [1-9][0-9]*, failed 0$'; then
    ok "standards-gate-test"
  else
    bad "standards-gate-test" "gate test exited 0 without a passing summary: $summary"
  fi
else
  bad "standards-gate-test" "gate test failed: $(tail -n 3 "$tmpdir/gate-test.out" | tr '\n' ' ')"
fi

# 5. The gate over canonical input, and the controls that prove it can fail.
gate="$tree/scripts/check-ijson-jcs.sh"
[ -f "$gate" ] || ijson_die "$IS_EXIT_WRONG_INPUT" "pinned gate script missing: $gate"
good_tree="$root/tests/fixtures/standards/good"
[ -d "$good_tree" ] || ijson_die "$IS_EXIT_CANNOT" "missing fixture tree: $good_tree"
expect_exit 0 "standards-gate-canonical-tree" \
  env IJSON_JCS="$binary" bash "$gate" --enforce "$good_tree"

mkdir -p "$tmpdir/planted"
printf '{"b":1,"a":2}\n' >"$tmpdir/planted/unsorted.json"
printf '{"a":1}\n\n{"b":2}\n' >"$tmpdir/planted/blank-line.jsonl"
expect_exit 1 "standards-gate-detects-non-canonical" \
  env IJSON_JCS="$binary" bash "$gate" --enforce "$tmpdir/planted"
expect_exit 1 "standards-gate-detects-planted-json-only" \
  env IJSON_JCS="$binary" bash "$gate" --enforce "$tmpdir/planted/unsorted.json"
expect_exit 2 "standards-gate-missing-tool" \
  env IJSON_JCS=ijson-jcs-definitely-not-installed bash "$gate" "$good_tree"
expect_exit 2 "standards-gate-missing-path" \
  env IJSON_JCS="$binary" bash "$gate" "$tmpdir/does-not-exist"

# This repository's own machine-readable JSON is held to J-1 as well.
own_paths=()
for own in "$root/.ci" "$root/interop"; do
  if [ -d "$own" ]; then
    own_paths+=("$own")
  fi
done
if [ "${#own_paths[@]}" -gt 0 ]; then
  expect_exit 0 "standards-gate-own-json" \
    env IJSON_JCS="$binary" bash "$gate" --enforce "${own_paths[@]}"
fi

# 6. The profile matrix, JSON Lines framing and JSON-LD scope, via the binary.
profile_matrix() {
  local failures=0
  local name bytes want_library want_estate want_tolerant code
  while IFS='|' read -r name bytes want_library want_estate want_tolerant; do
    printf '%b' "$bytes" >"$tmpdir/case.json"
    for pair in "library:$want_library" "estate-file:$want_estate" "tolerant:$want_tolerant"; do
      code=0
      "$binary" check --profile "${pair%%:*}" -- "$tmpdir/case.json" >/dev/null 2>&1 || code=$?
      if [ "$code" != "${pair##*:}" ]; then
        printf 'profile %s on %s: wanted %s, got %s\n' "${pair%%:*}" "$name" "${pair##*:}" "$code" >&2
        failures=$((failures + 1))
      fi
    done
  done <<'MATRIX'
bare-canonical|{"a":1}|0|1|0
one-final-lf|{"a":1}\n|1|0|0
two-final-lf|{"a":1}\n\n|1|1|1
crlf|{"a":1}\r\n|1|1|1
leading-space| {"a":1}\n|1|1|1
unsorted|{"b":1,"a":2}\n|1|1|1
duplicate-key|{"a":1,"a":2}\n|2|2|2
truncated|{"a":1\n|2|2|2
MATRIX
  return "$failures"
}
if profile_matrix; then
  ok "estate-file-profile-matrix"
else
  bad "estate-file-profile-matrix" "the candidate binary disagrees with JSON-POLICY J-1"
fi

jsonl_matrix() {
  local failures=0 want code
  mkdir -p "$tmpdir/records"
  printf '{"a":1}\n{"b":[1,2]}\n' >"$tmpdir/records/good.jsonl"
  printf '{"a":1}\n{"b":[1,2]}' >"$tmpdir/records/no-final-lf.jsonl"
  printf '{"a":1}\n{"z":1,"b":2}\n' >"$tmpdir/records/unsorted.jsonl"
  printf '{"a":1}\n\n{"b":2}\n' >"$tmpdir/records/blank-line.jsonl"
  printf '{"a":1}\n{"b":\n' >"$tmpdir/records/truncated.jsonl"
  printf '{"a":1}\n{"a":1,"a":2}\n' >"$tmpdir/records/duplicate.jsonl"
  while IFS='|' read -r file want; do
    code=0
    "$binary" check-jsonl -- "$tmpdir/records/$file" >/dev/null 2>&1 || code=$?
    if [ "$code" != "$want" ]; then
      printf 'check-jsonl %s: wanted %s, got %s\n' "$file" "$want" "$code" >&2
      failures=$((failures + 1))
    fi
  done <<'MATRIX'
good.jsonl|0
no-final-lf.jsonl|1
unsorted.jsonl|1
blank-line.jsonl|2
truncated.jsonl|2
duplicate.jsonl|2
MATRIX
  return "$failures"
}
if jsonl_matrix; then
  ok "jsonl-record-framing"
else
  bad "jsonl-record-framing" "the candidate binary disagrees with JSON-POLICY J-2"
fi

jsonld_scope() {
  local want='{"@context":{"name":"https://schema.org/name"},"@id":"https://example.invalid/alice","name":"Alice"}'
  local got
  got="$(printf '{"name":"Alice","@id":"https://example.invalid/alice","@context":{"name":"https://schema.org/name"}}' |
    "$binary" jsonld)" || return 1
  [ "$got" = "$want" ]
}
if jsonld_scope; then
  ok "jsonld-document-scope"
else
  bad "jsonld-document-scope" "jsonld did not print the document's JCS bytes"
fi

# 7. Differential vectors for kyq, when they are present. Their absence is a
#    finding, not a pass: the consumer contract is part of this pipeline.
if [ -d "$root/interop/kyq/vectors" ]; then
  if bash "$scripts_dir/kyq-vectors.sh" verify --root "$root" --binary "$binary" >"$tmpdir/vectors.out" 2>&1; then
    ok "kyq-differential-vectors"
  else
    # Name the cases, not a wall of bytes: this has to stay readable inside one
    # CI annotation, and the case id is what a reviewer acts on.
    bad "kyq-differential-vectors" \
      "$(grep -E '^(FAIL|kyq-vectors:)' "$tmpdir/vectors.out" | head -n 6 | tr '\n' ';' | cut -c1-1400)"
  fi
else
  bad "kyq-differential-vectors" "interop/kyq/vectors is missing"
fi

# 8. The receipt. Written last, because a receipt records success.
write_receipt() {
  local out="$1"
  local manifest_sha contract_sha toolchain_sha rustc_version cargo_version source_commit dirty
  manifest_sha="$(ijson_sha256 "$root/.ci/standards.sha256")"
  contract_sha="$(ijson_sha256 "$root/.ci/standards-contract.tsv")"
  toolchain_sha="$(ijson_sha256 "$root/rust-toolchain.toml")"
  rustc_version="unknown"
  cargo_version="unknown"
  if command -v rustc >/dev/null 2>&1; then
    rustc_version="$(rustc --version 2>/dev/null | awk '{print $2}')"
  fi
  if command -v cargo >/dev/null 2>&1; then
    cargo_version="$(cargo --version 2>/dev/null | awk '{print $2}')"
  fi
  source_commit="unknown"
  dirty=false
  if command -v git >/dev/null 2>&1; then
    candidate="$(git -C "$root" rev-parse HEAD 2>/dev/null || true)"
    if [ -n "$candidate" ]; then
      source_commit="$candidate"
    fi
    if [ -n "$(git -C "$root" status --porcelain 2>/dev/null | head -n 1)" ]; then
      dirty=true
    fi
  fi
  local checks_json
  checks_json="$(jq -cn --args '$ARGS.positional | map({name: ., status: "pass"})' "${names[@]}")"
  case "$out" in
    */*) mkdir -p -- "${out%/*}" ;;
  esac
  jq -cS -n \
    --arg schema "ijson-jcs/standards-receipt/1" \
    --arg generated "$(ijson_utc_now)" \
    --arg repository "metadatastician/standards" \
    --arg revision "$rev" \
    --arg manifest_sha256 "$manifest_sha" \
    --arg contract_sha256 "$contract_sha" \
    --arg rustc "$rustc_version" \
    --arg cargo "$cargo_version" \
    --arg toolchain_sha256 "$toolchain_sha" \
    --arg source_commit "$source_commit" \
    --arg label "$label" \
    --arg path "${binary#"$root"/}" \
    --arg binary_sha256 "$(ijson_sha256 "$binary")" \
    --argjson dirty "$dirty" \
    --argjson checks "$checks_json" \
    --argjson pinned "$(ijson_manifest_entries "$root/.ci/standards.sha256" | wc -l | tr -d ' ')" \
    '{
      schema: $schema,
      generated: $generated,
      label: $label,
      source: {commit: $source_commit, dirty: $dirty},
      standards: {
        repository: $repository,
        revision: $revision,
        pinned_files: $pinned,
        manifest_sha256: $manifest_sha256,
        contract_sha256: $contract_sha256
      },
      toolchain: {cargo: $cargo, rustc: $rustc, toolchain_file_sha256: $toolchain_sha256},
      binary: {path: $path, sha256: $binary_sha256},
      checks: $checks,
      verdict: "accepted"
    }' >"$out.tmp"
  mv -- "$out.tmp" "$out"
}

if [ "$failed" -ne 0 ]; then
  printf 'verify: %d passed, %d failed at %s; no receipt written\n' "$passed" "$failed" "$rev" >&2
  exit "$IS_EXIT_FINDING"
fi

if [ -n "$receipt" ]; then
  write_receipt "$receipt"
  expect_exit 0 "receipt-is-an-estate-file" \
    "$binary" check --profile estate-file -- "$receipt"
  if [ "$failed" -ne 0 ]; then
    # A receipt that is not itself an estate file is not evidence. Move it out
    # of the way rather than leaving a file that says "accepted" and is not.
    mv -- "$receipt" "$receipt.rejected" 2>/dev/null || true
    printf 'verify: the receipt failed its own estate-file check; moved aside, nothing accepted\n' >&2
    exit "$IS_EXIT_FINDING"
  fi
fi

printf 'verify: %d passed, %d failed at %s\n' "$passed" "$failed" "$rev"
