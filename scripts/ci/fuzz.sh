#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
source .ci/pins.sh
seconds=${1:-30}
[[ "$seconds" =~ ^[1-9][0-9]*$ ]] && (( seconds <= 3600 )) || { echo 'invalid fuzz duration' >&2; exit 2; }
rustup toolchain install "$FUZZ_TOOLCHAIN" --profile minimal
cargo install cargo-fuzz --version "$CARGO_FUZZ_VERSION" --locked
# cargo-fuzz 0.13.2 has no --locked flag. Preflight with locked Cargo, run
# offline, then require byte-identical locks; never invent an unsupported flag.
cargo +"$FUZZ_TOOLCHAIN" fetch --locked --manifest-path fuzz/Cargo.toml
before=$(sha256sum Cargo.lock fuzz/Cargo.lock)
mkdir -p target/ci/fuzz-corpus
for target in parse numbers; do
  cp -R "fuzz/corpus/$target" "target/ci/fuzz-corpus/$target"
  status=0
  CARGO_NET_OFFLINE=true cargo +"$FUZZ_TOOLCHAIN" fuzz run "$target" "target/ci/fuzz-corpus/$target" -- \
    -max_total_time="$seconds" -max_len=65536 -rss_limit_mb=2048 -timeout=5 -seed=1229606734 \
    2>&1 | tee "target/ci/fuzz-$target.log" || status=$?
  if [[ -d fuzz/artifacts ]]; then cp -R fuzz/artifacts target/ci/fuzz-artifacts; fi
  (( status == 0 )) || exit "$status"
  grep -Eq '^#[1-9][0-9]*[[:space:]]+DONE' "target/ci/fuzz-$target.log"
done
[[ "$(sha256sum Cargo.lock fuzz/Cargo.lock)" == "$before" ]] || { echo 'fuzz changed a lockfile' >&2; exit 2; }
