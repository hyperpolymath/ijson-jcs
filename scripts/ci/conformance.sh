#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Invoke canonical upstream validators; never vend a lookalike local validator.
set -euo pipefail
source .ci/pins.sh
standards=$(bash scripts/ci/fetch-source.sh "standards-$STANDARDS_REV" "$STANDARDS_REPOSITORY" "$STANDARDS_REV")
cargo build --locked --bin ijson-jcs
export IJSON_JCS=$PWD/target/debug/ijson-jcs
bash scripts/ci/standards.sh
files=()
while IFS= read -r -d '' file; do files+=("$file"); done < <(git ls-files -z '*.json' '*.jsonld' '*.json-ld' '*.jsonl' '*.ndjson')
if (( ${#files[@]} > 0 )); then
  bash scripts/check-estate-json.sh "${files[@]}"
else
  echo 'No tracked JSON-family payloads; non-vacuous framing fixtures ran separately.'
fi
bun "$standards/1-formats/deed/tools/deed_lint.js" --self-test
bun "$standards/1-formats/deed/tools/deed_lint.js" --fixtures "$standards/1-formats/deed/tools/fixtures"
bun "$standards/1-formats/deed/tools/deed_lint.js" 0-AI-MANIFEST.deed
validator=$standards/1-formats/k9/tools/k9-validate.sh
bash "$validator" --self-test
bash "$validator" --layer L2 --strict --fixtures "$standards/1-formats/k9/tools/fixtures"
bash "$validator" --layer L2 --strict ci.k9.ncl
cargo run --locked --quiet --example number_corpus > target/ci/number-corpus.jsonl
bun tests/fixtures/ecmascript-oracle.mjs target/ci/number-corpus.jsonl
# The data-only Kennel contract has no signature. L3 is not claimed or granted.
echo 'DEED and K9 L0-L2 checked; L3 cryptographic authorization is not applicable to this data-only component.'
