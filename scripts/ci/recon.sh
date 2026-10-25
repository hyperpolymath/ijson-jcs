#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Static, read-only analysis only: no dynamic attacks against external systems.
set -euo pipefail
source .ci/pins.sh
root=${IJSON_TOOLS_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/ijson-tools}
pons_source=$(bash scripts/ci/fetch-source.sh pons-asinorum hyperpolymath/pons-asinorum "$PONS_REV")
panic_source=$(bash scripts/ci/fetch-source.sh panic-attack hyperpolymath/panic-attack "$PANIC_REV")
cargo install --locked --path "$pons_source/crates/pons-cli" --root "$root/pons-bin"
cargo install --locked --path "$panic_source" --root "$root/panic-bin" --no-default-features
pons=$root/pons-bin/bin/pons
panic=$root/panic-bin/bin/panic-attack
mkdir -p target/ci
cargo build --locked --bin ijson-jcs
status=0
# Full inventory: unsupported languages stay visible in the report. Rust
# production scope separately requires complete coverage with no skipped files.
"$pons" scan . --format json --require-language rust --fail-on error > target/ci/pons-repository.raw || status=$?
(( status <= 1 )) || { echo 'pons could not scan the repository' >&2; exit 2; }
"$pons" validate-report target/ci/pons-repository.raw
source_status=0
"$pons" scan src --format json --require-language rust --fail-on-skips --fail-on error > target/ci/pons-rust.raw || source_status=$?
(( source_status <= 1 )) || { echo 'pons Rust coverage is incomplete' >&2; exit 2; }
"$pons" validate-report target/ci/pons-rust.raw
expected_files=()
while IFS= read -r -d '' file; do expected_files+=("$file"); done < <(git ls-files -z 'src/*.rs')
(( ${#expected_files[@]} > 0 )) || { echo 'no tracked production Rust' >&2; exit 2; }
jq -e --argjson expected "${#expected_files[@]}" '.scanned.files == $expected and .execution.status == "completed" and .scanned.files > 0 and (.scanned.skipped | length) == 0 and (.scanned.coverage_failures | length) == 0' target/ci/pons-rust.raw >/dev/null
"$panic" --quiet assail . --headless --output target/ci/panic-attack.raw > target/ci/panic-attack.stdout
# assail's exit 0 only means it executed, NOT that no vulnerabilities exist.
jq -e -s 'length == 1 and (.[0] |
  .schema_version == "2.5" and (.file_statistics | length) > 0 and
  (.weak_points | type == "array") and all(.weak_points[];
    (.severity == "Low" or .severity == "Medium" or .severity == "High" or .severity == "Critical") and
    (.description | type == "string")))' target/ci/panic-attack.raw >/dev/null
for tool in pons-repository pons-rust panic-attack; do
  bash scripts/ci/canonical-report.sh "target/ci/$tool.raw" "target/ci/$tool.json"
done
jq '{scanned: .scanned.files, coverage: .scanned.coverage_status, findings: .counts}' target/ci/pons-repository.json
jq '{files: (.file_statistics | length), unsuppressed: [.weak_points[] | select(.suppressed != true) | {severity, category, file, line, description}]}' target/ci/panic-attack.json
# Never invent a clean baseline. Tests/scanner context suppressions remain in
# the raw report; every non-suppressed High/Critical finding blocks this lane.
jq -e 'all(.weak_points[]; .suppressed == true or (.severity != "High" and .severity != "Critical"))' target/ci/panic-attack.json >/dev/null
summary=$(jq -c --slurpfile rust target/ci/pons-rust.json '{panic_files:(.file_statistics|length),
  unsuppressed_high_critical:([.weak_points[]|select(.suppressed != true and (.severity == "High" or .severity == "Critical"))]|length),
  pons_rust_files:$rust[0].scanned.files,pons_rust_skipped:($rust[0].scanned.skipped|length)}' target/ci/panic-attack.json)
printf '::notice title=Recon coverage::%s\n' "$summary"
(( status == 0 && source_status == 0 ))
