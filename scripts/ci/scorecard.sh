#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Repository posture assessment. Unknown/unavailable checks remain visible.
set -euo pipefail
source .ci/pins.sh
[[ "$(uname -s)/$(uname -m)" == Linux/x86_64 ]] || { echo 'supported on Linux/x86_64 CI only' >&2; exit 2; }
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
asset=scorecard_${SCORECARD_VERSION}_linux_amd64.tar.gz
gh release download "v$SCORECARD_VERSION" --repo ossf/scorecard --pattern "$asset" --dir "$work"
printf '%s  %s\n' "$SCORECARD_SHA256" "$work/$asset" | sha256sum --check --status
tar -xzf "$work/$asset" -C "$work"
mkdir -p target/ci
"$work/scorecard" --repo="github.com/${GITHUB_REPOSITORY:-hyperpolymath/ijson-jcs}" --format=json --show-details > target/ci/scorecard.raw
jq -e '(.checks | type == "array" and length > 0) and all(.checks[]; (.name | type == "string") and (.score | type == "number"))' target/ci/scorecard.raw >/dev/null
cargo build --locked --bin ijson-jcs
bash scripts/ci/canonical-report.sh target/ci/scorecard.raw target/ci/scorecard.json
jq '{repo,checks:[.checks[]|{name,score,reason}]}' target/ci/scorecard.json
echo 'Scorecard is an advisory repository assessment; -1 means unavailable, not passed. No public badge/dataset publication.'
