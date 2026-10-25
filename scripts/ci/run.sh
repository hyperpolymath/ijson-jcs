#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Preserve exit status and evidence. An empty command is a harness error.
set -euo pipefail
label=${1:?usage: run.sh LABEL COMMAND...}
shift
(( $# > 0 )) || { echo 'usage: run.sh LABEL COMMAND...' >&2; exit 2; }
[[ "$label" =~ ^[a-z0-9_-]+$ ]] || { echo 'invalid log label' >&2; exit 2; }
command -v jq >/dev/null || { echo 'jq is required for execution receipts' >&2; exit 2; }
log_dir=${IJSON_EVIDENCE_DIR:-target/ci}
mkdir -p "$log_dir"
started=$SECONDS
status=0
"$@" 2>&1 | tee "$log_dir/$label.log" || status=$?
elapsed=$((SECONDS - started))
log_hash=$(sha256sum "$log_dir/$label.log"); log_hash=${log_hash%% *}
revision=$(git rev-parse HEAD)
toolchain=$(rustc --version 2>/dev/null || printf 'unavailable')
jq -cn --arg task "$label" --arg source "$revision" --arg toolchain "$toolchain" \
  --arg log_sha256 "$log_hash" --argjson exit_code "$status" --argjson seconds "$elapsed" \
  --args '{schema_version:1, provenance:"Unproven", "label":$task, source:$source,
    toolchain:$toolchain, argv:$ARGS.positional, exit_code:$exit_code,
    elapsed_seconds:$seconds, log_sha256:$log_sha256,
    status:(if $exit_code == 0 then "passed" else "failed" end)}' -- "$@" \
  > "$log_dir/$label.receipt.raw"
if (( status != 0 )); then
  message=$(tail -c 3000 "$log_dir/$label.log")
  message=${message//'%'/'%25'}
  message=${message//$'\r'/'%0D'}
  message=${message//$'\n'/'%0A'}
  printf '::error title=%s (exit %d)::%s\n' "$label" "$status" "$message"
fi
exit "$status"
