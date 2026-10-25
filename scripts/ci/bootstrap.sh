#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Branch bootstrap only; removed once the resulting lock/format patch is applied.
set -euo pipefail
cargo generate-lockfile
cargo fmt --all
payload=$(git diff -- Cargo.lock src tests benches examples | base64 -w0)
# Keep notices under Checks' per-annotation size limit.
for (( offset=0; offset<${#payload}; offset+=50000 )); do
  printf '::notice title=bootstrap-%06d::%s\n' "$offset" "${payload:offset:50000}"
done
