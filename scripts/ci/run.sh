#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Preserve the command's exit code and expose a bounded diagnostic via Checks.
set -euo pipefail
label=${1:?usage: run.sh LABEL COMMAND...}
shift
(( $# > 0 )) || { echo 'usage: run.sh LABEL COMMAND...' >&2; exit 2; }
[[ "$label" =~ ^[a-z0-9_-]+$ ]] || { echo 'invalid log label' >&2; exit 2; }
mkdir -p target/ci
status=0
"$@" 2>&1 | tee "target/ci/$label.log" || status=$?
if (( status != 0 )); then
  message=$(tail -c 6000 "target/ci/$label.log")
  message=${message//'%'/'%25'}
  message=${message//$'\r'/'%0D'}
  message=${message//$'\n'/'%0A'}
  printf '::error title=%s (exit %d)::%s\n' "$label" "$status" "$message"
fi
exit "$status"
