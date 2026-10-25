#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Read-only test job. Produces a reviewable patch, never commits or pushes it.
set -euo pipefail
source .ci/pins.sh
candidate=${CANDIDATE_REVISION:-$(gh api "repos/$STANDARDS_REPOSITORY/commits/main" --jq .sha)}
[[ "$candidate" =~ ^[0-9a-f]{40}$ ]] || { echo 'candidate must be a full commit SHA' >&2; exit 2; }
mkdir -p target/ci/proposal
if [[ "$candidate" == "$STANDARDS_REV" ]]; then
  printf 'changed=false\n' >> "${GITHUB_OUTPUT:-/dev/stdout}"
  printf 'No standards revision change: %s\n' "$candidate"
  exit 0
fi
cargo test --locked --all-features
cargo test --locked --release --all-features
cargo build --locked --bin ijson-jcs
export IJSON_JCS=$PWD/target/debug/ijson-jcs
before=$(sha256sum "$IJSON_JCS")
bash scripts/ci/standards.sh
bash scripts/ci/standards.sh "$candidate"
[[ "$(sha256sum "$IJSON_JCS")" == "$before" && -z "$(git status --porcelain)" ]] || {
  echo "Candidate fixtures modified the implementation or binary" >&2; exit 2;
}
source=$(bash scripts/ci/fetch-source.sh "standards-$candidate" "$STANDARDS_REPOSITORY" "$candidate")
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
mkdir -p "$work/a/.ci" "$work/b/.ci"
cp .ci/pins.sh .ci/standards.sha256 "$work/a/.ci/"
grep -qFx "readonly STANDARDS_REV='$STANDARDS_REV'" .ci/pins.sh
sed "s/^readonly STANDARDS_REV='$STANDARDS_REV'$/readonly STANDARDS_REV='$candidate'/" .ci/pins.sh > "$work/b/.ci/pins.sh"
while read -r _ path; do
  (cd "$source" && sha256sum "$path")
done < .ci/standards.sha256 > "$work/b/.ci/standards.sha256"
bash scripts/ci/standard-inputs.sh "$source" "$work/b/.ci/standards.sha256"
status=0
(cd "$work" && git diff --no-index --no-prefix a b) > target/ci/proposal/standards.patch || status=$?
(( status == 1 )) || { echo 'expected a non-empty, generated proposal diff' >&2; exit 2; }
git apply --check target/ci/proposal/standards.patch
binary_hash=$(sha256sum "$IJSON_JCS"); binary_hash=${binary_hash%% *}
jq -cn --arg previous "$STANDARDS_REV" --arg candidate "$candidate" \
  --arg source "$(git rev-parse HEAD)" --arg binary_sha256 "$binary_hash" --arg toolchain "$(rustc --version)" \
  '{schema_version:1,status:"tested-proposal",previous:$previous,candidate:$candidate,
    implementation:$source,binary_sha256:$binary_sha256,toolchain:$toolchain,
    provenance:"Unproven",promotion:"owner-reviewed signed promotion required"}' > target/ci/proposal/receipt.raw
bash scripts/ci/canonical-report.sh target/ci/proposal/receipt.raw target/ci/proposal/receipt.json
printf 'changed=true\ncandidate=%s\n' "$candidate" >> "${GITHUB_OUTPUT:-/dev/stdout}"
