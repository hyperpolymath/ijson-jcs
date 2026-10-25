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
cargo audit --db "$database" --no-fetch --json > target/ci/audit.raw
cargo build --locked --bin ijson-jcs
bash scripts/ci/canonical-report.sh target/ci/audit.raw target/ci/audit.json
