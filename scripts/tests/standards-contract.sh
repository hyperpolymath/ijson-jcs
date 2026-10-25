#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Independent controls prevent a candidate upstream fixture suite going vacuous.
set -euo pipefail
standards=${1:?standards root required}
export IJSON_JCS=${2:?candidate canonicalizer required}
checker=$standards/scripts/check-ijson-jcs.sh
[[ -x "$checker" && -x "$IJSON_JCS" ]] || { echo 'required checker/binary missing' >&2; exit 2; }
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
mkdir "$work/tree"
expect() {
  local want=$1 marker=$2 got=0
  bash "$checker" --enforce "$work/tree" > "$work/log" 2>&1 || got=$?
  if (( got != want )) || ! grep -qF "$marker" "$work/log"; then
    cat "$work/log"; echo "candidate contract drift: expected $want / $marker" >&2; exit 1
  fi
}
printf '{"a":1}\n' > "$work/tree/input.json"
expect 0 '1 canonical'
printf '{"a":1}' > "$work/tree/input.json"
expect 1 'NOT CANONICAL'
printf '{"a":1,"a":2}\n' > "$work/tree/input.json"
expect 1 'INVALID'
rm "$work/tree/input.json"
printf '{"a":1}\n{"b":2}\n' > "$work/tree/input.jsonl"
expect 0 '1 canonical'
printf '{"a":1}\n\n' > "$work/tree/input.jsonl"
expect 1 'INVALID'
printf '{"a":1}\n{"b":2}' > "$work/tree/input.jsonl"
expect 1 'NOT CANONICAL'
echo 'Candidate standards retain the independently tested estate JSON contract'
