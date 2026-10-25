#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
files=(.ci/pins.sh)
while IFS= read -r -d '' file; do files+=("$file"); done < <(git ls-files -z 'scripts/*.sh')
(( ${#files[@]} > 1 )) || { echo 'No Bash scripts checked' >&2; exit 2; }
for file in "${files[@]}"; do bash -n "$file"; done
shellcheck --shell=bash --external-sources --severity=warning "${files[@]}"
