#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
source .ci/pins.sh
cargo install cargo-audit --version "$CARGO_AUDIT_VERSION" --locked
# Advisory DATA is intentionally refreshed, then frozen to a recorded revision.
revision=$(gh api repos/RustSec/advisory-db/commits/HEAD --jq .sha)
database=$(bash scripts/ci/fetch-source.sh "rustsec-$revision" RustSec/advisory-db "$revision")
mkdir -p target/ci
printf '%s\n' "$revision" > target/ci/rustsec-revision.txt
cargo build --locked --bin ijson-jcs
status=0
for item in library:Cargo.lock fuzz:fuzz/Cargo.lock; do
  name=${item%%:*}; lock=${item#*:}
  [[ -s "$lock" ]] || { echo "missing lockfile: $lock" >&2; exit 2; }
  cargo audit --file "$lock" --db "$database" --no-fetch --json > "target/ci/audit-$name.raw" || status=$?
  bash scripts/ci/canonical-report.sh "target/ci/audit-$name.raw" "target/ci/audit-$name.json"
  jq -e '.lockfile."dependency-count" > 0 and .vulnerabilities.found == false' "target/ci/audit-$name.json" >/dev/null
done
exit "$status"
