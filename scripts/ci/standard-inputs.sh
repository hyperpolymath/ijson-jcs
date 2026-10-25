#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Verify required normative files. Exit 1: content drift; 2: unable to check.
set -euo pipefail
root=${1:?source root required} lock=${2:?SHA256 manifest required}
[[ -d "$root" && -s "$lock" ]] || { echo 'missing standards source/manifest' >&2; exit 2; }
root=$(realpath "$root")
declare -A seen=()
count=0
while read -r digest path extra; do
  [[ "$digest" =~ ^[0-9a-f]{64}$ && "$path" =~ ^[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)*$ && -z "$extra" ]] || {
    echo 'malformed standards manifest' >&2; exit 2;
  }
  case "/$path/" in */../*|*/./*) echo 'unsafe standards path' >&2; exit 2;; esac
  [[ -f "$root/$path" && ! -L "$root/$path" ]] || { echo "missing normative input: $path" >&2; exit 2; }
  [[ "$(realpath "$root/$path")" == "$root/"* && -z "${seen[$path]:-}" ]] || {
    echo "escaped or repeated normative input: $path" >&2; exit 2;
  }
  seen[$path]=1
  count=$((count + 1))
done < "$lock"
(( count > 0 )) || { echo 'empty normative input set' >&2; exit 2; }
# Resolve the manifest before changing directory; spaces in the root are safe.
lock=$(realpath "$lock")
(cd "$root" && sha256sum --strict --check "$lock")
