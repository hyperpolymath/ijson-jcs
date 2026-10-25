#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Execute the ACTUAL aggregate predicate, not a separate lookalike test gate.
set -euo pipefail
lanes=$(yq -p yaml -o json '.jobs.required.needs' .github/workflows/ci.yml)
command=$(yq -p yaml -r '.jobs.required.steps[0].run' .github/workflows/ci.yml)
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
jq -n --argjson lanes "$lanes" '$lanes | map({key:.,value:{result:"success"}}) | from_entries' > "$work/clean"
NEEDS=$(cat "$work/clean") bash -euo pipefail -c "$command" > "$work/log" 2>&1
while IFS= read -r lane; do
  for result in failure skipped cancelled neutral timed_out action_required unknown; do
    json=$(jq --arg lane "$lane" --arg result "$result" '.[$lane].result = $result' "$work/clean")
    if NEEDS="$json" bash -euo pipefail -c "$command" > "$work/log" 2>&1; then
      echo "aggregate accepted $lane = $result" >&2; exit 1
    fi
  done
  json=$(jq --arg lane "$lane" 'del(.[$lane])' "$work/clean")
  if NEEDS="$json" bash -euo pipefail -c "$command" > "$work/log" 2>&1; then
    echo "aggregate accepted missing $lane" >&2; exit 1
  fi
done < <(jq -r '.[]' <<< "$lanes")
if NEEDS='{}' bash -euo pipefail -c "$command" > "$work/log" 2>&1; then
  echo 'aggregate accepted empty needs' >&2; exit 1
fi
echo 'Actual required gate rejects every failed/skipped/cancelled/missing lane'
