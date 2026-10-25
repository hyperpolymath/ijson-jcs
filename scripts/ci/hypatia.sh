#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Local rule coverage is explicit; forge-alert access is a separate permission.
set -euo pipefail
source .ci/pins.sh
hypatia=$(bash scripts/ci/fetch-source.sh hypatia hyperpolymath/hypatia "$HYPATIA_REV")
(
  cd "$hypatia"
  mix local.hex --force
  mix local.rebar --force
  mix deps.get
  git diff --exit-code -- mix.lock
  mix escript.build
)
test -x "$hypatia/hypatia"
cargo build --locked --bin ijson-jcs
mkdir -p target/ci
# Run the actual escript: no bash fallback or success placeholder.
"$hypatia/hypatia" scan "$PWD" --format json --severity info --exit-zero \
  --rules root_hygiene,honest_completion,workflow_audit,workflow_hardening,cicd_rules,code_safety,content_patterns > target/ci/hypatia.raw
jq -e -s 'length == 1 and (.[0] | type == "array" and all(.[];
  (.rule_module | type == "string") and (.severity as $s |
  ["critical","high","medium","warn","low","info","informational"] | index($s) != null)))' target/ci/hypatia.raw >/dev/null
bash scripts/ci/canonical-report.sh target/ci/hypatia.raw target/ci/hypatia.json
jq '.' target/ci/hypatia.json
jq -e 'all(.[]; .severity != "critical" and .severity != "high")' target/ci/hypatia.json >/dev/null
printf '%s\n' 'Hypatia local rules ran; Dependabot/secret/code-scanning API alert coverage is not claimed by this offline scope.'
