#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Validate actual YAML structure with yq; regular expressions only see values.
set -euo pipefail
source .ci/pins.sh
standards=$(bash scripts/ci/fetch-source.sh "standards-$STANDARDS_REV" "$STANDARDS_REPOSITORY" "$STANDARDS_REV")
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
git ls-files -z '*.yml' '*.yaml' '*.kyaml' > "$work/yaml-files"
files=()
while IFS= read -r -d '' file; do files+=("$file"); done < "$work/yaml-files"
(( ${#files[@]} > 0 )) || { echo 'No YAML files checked' >&2; exit 2; }
bash "$standards/scripts/kyaml-format.sh" --check "${files[@]}"
actionlint -color
for file in .github/workflows/*.yml; do
  yq -p yaml -o json '.' "$file" > "$work/workflow"
  jq -e '
    (.jobs | type == "object" and length > 0) and
    (.on | has("pull_request_target") | not) and
    (.permissions | type == "object" and all(.[]; . == "read" or . == "none")) and
    all(.jobs[]; if has("uses") then true else (."timeout-minutes" > 0 and ."timeout-minutes" <= 60) end) and
    all(.jobs[] | .uses?, .steps[]?.uses? | select(. != null);
      test("^[A-Za-z0-9_.-]+/[A-Za-z0-9_./-]+@[0-9a-f]{40}$") or
      . == "./.github/workflows/ci.yml") and
    all(.jobs[] | .steps[]? | select(.uses? | strings | startswith("actions/checkout@"));
      .with."persist-credentials" == false)
  ' "$work/workflow" >/dev/null
  echo "Workflow trust policy: $file"
done
# A required workflow must always start: no event-level file/branch filtering
# of pull_request or merge_group, and no skip/neutral promotion at aggregation.
yq -p yaml -o json '.' .github/workflows/ci.yml | jq -e '
  (.on | has("pull_request") and has("merge_group")) and
  ((.on.pull_request // {}) | has("paths") or has("paths-ignore") or has("branches") | not) and
  ((.on.merge_group // {}) | has("paths") or has("paths-ignore") | not) and
  (.jobs.required.if == "${{ always() }}") and
  (.jobs.required.needs | sort == (["quality","test","conformance","benchmarks","recon","hypatia","squabble","security","fuzz"] | sort)) and
  all(.jobs | to_entries[] | select(.key != "required"); .value.if == null)
' >/dev/null
