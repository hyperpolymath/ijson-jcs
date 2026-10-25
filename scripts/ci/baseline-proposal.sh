#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Fetch >=10 DISTINCT successful main-push runs; never manufacture run history.
# Produces a proposal under target/ci, not an automatically accepted baseline.
set -euo pipefail
source .ci/pins.sh
(( $# >= 10 )) || { echo 'BOOTSTRAPPING: ten genuine main run IDs required' >&2; exit 2; }
repo=hyperpolymath/ijson-jcs
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
declare -A seen=()
reports=()
for id in "$@"; do
  [[ "$id" =~ ^[1-9][0-9]*$ && -z "${seen[$id]:-}" ]] || { echo 'bad/duplicate run ID' >&2; exit 2; }
  seen[$id]=1
  gh api "repos/$repo/actions/runs/$id" > "$work/run"
  jq -e '.conclusion == "success" and .event == "push" and .head_branch == "main" and .path == ".github/workflows/ci.yml"' "$work/run" >/dev/null
  sha=$(jq -r .head_sha "$work/run")
  gh run download "$id" --repo "$repo" --name evidence-benchmarks-Linux-pinned --dir "$work/$id"
  file=$work/$id/proven-bench.json
  jq -e --arg sha "$sha" --arg id "$id" '.commit == $sha and .run_id == $id and .ref == "refs/heads/main" and .event == "push" and .subject == "ijson-jcs" and all(.results[]; .checksum == true)' "$file" >/dev/null
  reports+=("$file")
done
jq -s -e 'length >= 10 and (map([.workload_revision,.runner,.runner_arch,.rustc]) | unique | length) == 1' "${reports[@]}" >/dev/null
proven=$(bash scripts/ci/fetch-source.sh proven-tests-and-benches hyperpolymath/proven-tests-and-benches "$PROVEN_REV")
mkdir -p target/ci
bash "$proven/scripts/compute-bench-baseline.sh" "${reports[@]}" > target/ci/bench-baseline.raw
# The upstream tolerance calibration belongs to its Idris workloads, not ours.
jq --arg rev "$PROVEN_REV" '.upstream_tolerance_note = .note |
  .note = "Uses upstream 2.0x relative / 2.5x absolute thresholds. These are imported policy, not a calibration result for ijson-jcs; owner review is required." |
  .statistics_tool_revision = $rev' target/ci/bench-baseline.raw > target/ci/bench-baseline-proposal.raw
cargo build --locked --bin ijson-jcs
bash scripts/ci/canonical-report.sh target/ci/bench-baseline-proposal.raw target/ci/bench-baseline-proposal.json
echo 'Review source runs, workload/hardware comparability and tolerances before accepting .ci/bench-baseline.json'
