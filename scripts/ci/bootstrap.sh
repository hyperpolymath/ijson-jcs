#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Branch bootstrap only; removed once the resulting lock/format patch is applied.
set -euo pipefail
cargo generate-lockfile
cargo fmt --all
cargo generate-lockfile --manifest-path fuzz/Cargo.toml
cargo fmt --manifest-path fuzz/Cargo.toml --all
git add -N fuzz/Cargo.lock
if command -v yq >/dev/null; then
  source .ci/pins.sh
  standards=$(bash scripts/ci/fetch-source.sh "standards-$STANDARDS_REV" "$STANDARDS_REPOSITORY" "$STANDARDS_REV")
  files=()
  while IFS= read -r -d '' file; do files+=("$file"); done < <(git ls-files -z '*.yml' '*.yaml' '*.kyaml')
  bash "$standards/scripts/kyaml-format.sh" "${files[@]}"
fi
payload=$(git diff -- Cargo.lock src tests benches examples fuzz .github | gzip -c | base64 -w0)
# Keep notices under Checks' per-annotation size limit.
for (( offset=0; offset<${#payload}; offset+=3000 )); do
  printf '::notice title=bootstrap-%06d::%s\n' "$offset" "${payload:offset:3000}"
done
