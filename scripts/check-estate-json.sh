#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Transitional four-format routing: this repo's ijson-jcs, never jq as a canon.
# Only JSON-family bytes are checked; JSON-LD contexts are never fetched.
set -euo pipefail
binary=${IJSON_JCS:-ijson-jcs}
command -v "$binary" >/dev/null || { echo 'ijson-jcs is required' >&2; exit 2; }
(( $# > 0 )) || { echo 'usage: check-estate-json.sh FILE...' >&2; exit 2; }
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
status=0
for file in "$@"; do
  [[ -f "$file" ]] || { echo "missing file: $file" >&2; exit 2; }
  case "$file" in
    *.json|*.jsonld|*.json-ld)
      rc=0; "$binary" check -- "$file" || rc=$?
      if (( rc > 2 )); then echo 'canonicalizer failed operationally' >&2; exit 2; fi
      if (( rc != 0 )) || [[ ! -s "$file" || -n $(tail -c 1 "$file") ]]; then status=1; fi
      ;;
    *.jsonl|*.ndjson)
      if [[ ! -s "$file" || -n $(tail -c 1 "$file") ]]; then
        echo "INVALID FRAMING $file: non-empty records and final LF required" >&2
        status=1
      fi
      while IFS= read -r line || [[ -n "$line" ]]; do
        # Empty lines, CRLF and multiline records are never silently dropped.
        printf '%s' "$line" > "$work/input"
        rc=0; "$binary" canon < "$work/input" > "$work/canonical" || rc=$?
        if (( rc > 2 )); then echo 'canonicalizer failed operationally' >&2; exit 2; fi
        if (( rc != 0 )) || ! cmp -s "$work/input" "$work/canonical"; then
          echo "INVALID OR NONCANONICAL RECORD $file" >&2; status=1
        fi
      done < "$file"
      ;;
    *) echo "unsupported JSON-family extension: $file" >&2; exit 2;;
  esac
done
exit "$status"
