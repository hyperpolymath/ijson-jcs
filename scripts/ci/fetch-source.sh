#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Print a clean, verified checkout path. Never accept a floating ref/cache drift.
set -euo pipefail
name=${1:?name required} repo=${2:?owner/repo required} revision=${3:?revision required}
[[ "$name" =~ ^[a-z0-9-]+$ && "$repo" =~ ^[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+$ && "$revision" =~ ^[0-9a-f]{40}$ ]] || {
  echo 'fetch-source: invalid identity or non-immutable revision' >&2; exit 2;
}
root=${IJSON_TOOLS_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/ijson-tools}
destination=$root/$name
if [[ ! -e "$destination" ]]; then
  mkdir -p "$destination"
  git -C "$destination" init --quiet >&2
  git -C "$destination" remote add origin "https://github.com/$repo.git"
  git -C "$destination" fetch --quiet --no-tags --depth 1 origin "$revision" >&2
  git -C "$destination" checkout --quiet --detach FETCH_HEAD >&2
fi
[[ "$(git -C "$destination" rev-parse HEAD)" == "$revision" ]] || {
  echo "fetch-source: stale source at $destination" >&2; exit 2;
}
[[ "$(git -C "$destination" remote get-url origin)" == "https://github.com/$repo.git" ]] || {
  echo "fetch-source: wrong origin at $destination" >&2; exit 2;
}
git -C "$destination" diff --exit-code --quiet HEAD -- || { echo 'fetch-source: dirty source' >&2; exit 2; }
[[ -z "$(git -C "$destination" ls-files --others --exclude-standard)" ]] || {
  echo 'fetch-source: untracked source files' >&2; exit 2;
}
printf '%s\n' "$destination"
