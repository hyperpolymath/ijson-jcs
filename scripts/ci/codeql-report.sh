#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
bash scripts/tests/sarif-gate.sh
shopt -s nullglob
reports=(target/ci/codeql/*.sarif)
(( ${#reports[@]} > 0 )) || { echo 'CodeQL produced no SARIF' >&2; exit 2; }
cargo build --locked --bin ijson-jcs
status=0
for report in "${reports[@]}"; do
  bash scripts/ci/canonical-report.sh "$report" "${report%.sarif}.json"
  if ! jq -e -f scripts/ci/sarif-gate.jq "${report%.sarif}.json" >/dev/null; then
    status=1
    summary=$(jq -c '{version,runs:[.runs[]|{driver:.tool.driver.name,
      rule_count:(.tool.driver.rules|length),extension_rules:([.tool.extensions[]? | (.rules|length)]|add),
      result_count:(.results|length),property_keys:((.properties//{})|keys),
      metrics:.properties.metricResults,invocations:[.invocations[]?|{executionSuccessful}],
      results:[.results[]|{ruleId,level,message}]}]}' "${report%.sarif}.json")
    summary=${summary//'%'/'%25'}
    printf '::notice title=CodeQL gate diagnostics::%s\n' "$summary"
  fi
done
exit "$status"
