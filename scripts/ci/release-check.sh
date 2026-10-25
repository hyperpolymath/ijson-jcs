#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Fail closed before publication. No agent-signed or bypassed release.
set -euo pipefail
[[ ${GITHUB_REF_TYPE:-} == tag && ${GITHUB_REF_NAME:-} =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo 'Release requires a version tag, not a branch or arbitrary ref' >&2; exit 2;
}
tag=$GITHUB_REF_NAME
version=$(cargo metadata --locked --no-deps --format-version 1 | jq -er '.packages[] | select(.name == "ijson-jcs") | .version')
[[ "$tag" == "v$version" ]] || { echo 'tag/package version mismatch' >&2; exit 2; }
git fetch --no-tags origin main
git merge-base --is-ancestor HEAD origin/main
[[ "$(git cat-file -t "$tag")" == tag ]] || { echo 'an annotated, signed tag is required' >&2; exit 2; }
[[ "$(git rev-parse "$tag^{commit}")" == "$(git rev-parse HEAD)" ]]
object=$(git rev-parse "$tag")
gh api "repos/$GITHUB_REPOSITORY/git/tags/$object" | jq -e '.verification.verified == true and .verification.reason == "valid"' >/dev/null
gh api "repos/$GITHUB_REPOSITORY/commits/$(git rev-parse HEAD)" | jq -e '.commit.verification.verified == true and .commit.verification.reason == "valid"' >/dev/null
# Merely naming an environment auto-creates an unprotected one. Inspect the
# real configuration; inaccessible configuration is not approval.
gh api "repos/$GITHUB_REPOSITORY/environments/release" | jq -e -f scripts/ci/release-policy.jq >/dev/null
[[ -z "$(git status --porcelain)" ]] || { echo 'release source is dirty' >&2; exit 2; }
IJSON_REQUIRE_BASELINE=1 bash scripts/ci/benchmarks.sh
cargo package --locked
mkdir -p target/ci/release
cp "target/package/ijson-jcs-$version.crate" target/ci/release/
(cd target/ci/release && sha256sum "ijson-jcs-$version.crate" > SHA256SUMS)
printf 'Source-only draft release from %s. No crates.io publication is performed.\n' "$(git rev-parse HEAD)" > target/ci/release/NOTES.md
