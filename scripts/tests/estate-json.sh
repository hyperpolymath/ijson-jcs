#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Silence/firing controls for framing and JSON-LD byte-only behavior.
set -euo pipefail
export IJSON_JCS=${1:?ijson-jcs binary required}
root=$PWD
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
expect() {
  local want=$1; shift
  local got=0
  "$@" > "$work/last.log" 2>&1 || got=$?
  if (( got != want )); then cat "$work/last.log"; echo "wanted $want, got $got" >&2; exit 1; fi
}
for extension in jsonl ndjson; do
  file="$work/records.$extension"
  printf '{"a":1}\n{"b":2}\n' > "$file"
  expect 0 bash "$root/scripts/check-estate-json.sh" "$file"
  for malformed in '{"a":1}' $'{"a":1}\n\n' $'{"a":1}\r\n' $'{\n"a":1}\n' $'{"a":1}\n{"b":1,"b":2}\n' ''; do
    printf '%s' "$malformed" > "$file"
    expect 1 bash "$root/scripts/check-estate-json.sh" "$file"
  done
done
printf '{"@context":"https://example.invalid/not-fetched","@id":"urn:example:x"}\n' > "$work/graph.jsonld"
expect 0 bash "$root/scripts/check-estate-json.sh" "$work/graph.jsonld"
printf '{"a":1}' > "$work/value.json"
expect 1 bash "$root/scripts/check-estate-json.sh" "$work/value.json"
printf '{"a":1}\n' > "$work/value.json"
expect 0 bash "$root/scripts/check-estate-json.sh" "$work/value.json"
expect 2 bash "$root/scripts/check-estate-json.sh" "$work/missing.json"
expect 2 bash "$root/scripts/check-estate-json.sh"
expect 2 env IJSON_JCS="$work/missing-binary" bash "$root/scripts/check-estate-json.sh" "$work/value.json"
echo 'Estate JSON controls: clean and faulty framing, missing inputs/tools verified'
