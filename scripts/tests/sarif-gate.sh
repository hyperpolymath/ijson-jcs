#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
jq -n '{version:"2.1.0",runs:[{tool:{driver:{name:"CodeQL",rules:[{id:"synthetic-control",properties:{"security-severity":"8.1"}}]}},results:[]}]}' > "$work/clean"
jq -e -f scripts/ci/sarif-gate.jq "$work/clean" >/dev/null
jq '.runs[0].tool.driver.rules[0].properties."security-severity"="3.0" | .runs[0].results=[{ruleId:"synthetic-control",level:"warning"}]' "$work/clean" > "$work/low"
jq -e -f scripts/ci/sarif-gate.jq "$work/low" >/dev/null
for fault in '.runs=[]' '.runs[0].tool.driver.rules=[]' \
  '.runs[0].results=[{ruleId:"synthetic-control"}]' \
  '.runs[0].results=[{ruleId:"unknown-rule"}]' \
  '.runs[0].results=[{ruleId:"synthetic-control"}] | .runs[0].tool.driver.rules[0].properties."security-severity"="NaN"'; do
  jq "$fault" "$work/clean" > "$work/fault"
  if jq -e -f scripts/ci/sarif-gate.jq "$work/fault" >/dev/null 2>&1; then
    echo "SARIF gate accepted $fault" >&2; exit 1
  fi
done
echo 'SARIF clean/high/missing-rule/empty-campaign controls passed'
