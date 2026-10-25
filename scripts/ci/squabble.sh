#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# The official pinned consumer action installs/attests squabble before this.
set -euo pipefail
mkdir -p target/ci
cargo build --locked --bin ijson-jcs
squabble chains . --follow 1 --json > target/ci/squabble-chains.raw
jq -e '.edge_count > 0 and (.scans | length) > 1 and (.frontier | length) == 0' target/ci/squabble-chains.raw >/dev/null
bash scripts/ci/canonical-report.sh target/ci/squabble-chains.raw target/ci/squabble-chains.json
# Deliberate no-bypass fixtures: a passed check and a missing required check
# must be distinct. They are models, NOT manufactured live GitHub gate state.
printf '{"checks":[{"required_context":"Required CI","run":"passed"}]}' > target/ci/gate-clean.input
printf '{"checks":[{"required_context":"Required CI","run":"missing"}]}' > target/ci/gate-missing.input
squabble diagnose target/ci/gate-clean.input > target/ci/gate-clean.txt
squabble diagnose target/ci/gate-missing.input > target/ci/gate-missing.txt
grep -q 'Green' target/ci/gate-clean.txt
if grep -q 'Green' target/ci/gate-missing.txt; then echo 'squabble calibrated a missing gate as green' >&2; exit 1; fi
printf '%s\n' 'Read-only CI chain analysis passed. Live verify-satisfied is a separate owner-authorized operation, not this model check.'
