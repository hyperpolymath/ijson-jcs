#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Linux CI-only bootstrap. Release assets are SHA256-verified BEFORE execution.
set -euo pipefail
source .ci/pins.sh
[[ "$(uname -s)/$(uname -m)" == Linux/x86_64 ]] || { echo 'provision: unsupported platform' >&2; exit 2; }
root=${IJSON_TOOLS_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/ijson-tools}
mkdir -p "$root/bin"
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
asset() {
  local repo=$1 tag=$2 name=$3 digest=$4
  gh release download "$tag" --repo "$repo" --pattern "$name" --dir "$work"
  printf '%s  %s\n' "$digest" "$work/$name" | sha256sum --check --status
}
case ${1:?usage: provision.sh yaml|conformance} in
  yaml|conformance)
    asset mikefarah/yq "v$YQ_VERSION" yq_linux_amd64 "$YQ_SHA256"
    install -m 755 "$work/yq_linux_amd64" "$root/bin/yq"
    asset rhysd/actionlint "v$ACTIONLINT_VERSION" "actionlint_${ACTIONLINT_VERSION}_linux_amd64.tar.gz" "$ACTIONLINT_SHA256"
    tar -xzf "$work/actionlint_${ACTIONLINT_VERSION}_linux_amd64.tar.gz" -C "$work" actionlint
    install -m 755 "$work/actionlint" "$root/bin/actionlint"
    asset koalaman/shellcheck "v$SHELLCHECK_VERSION" "shellcheck-v${SHELLCHECK_VERSION}.linux.x86_64.tar.xz" "$SHELLCHECK_SHA256"
    tar -xJf "$work/shellcheck-v${SHELLCHECK_VERSION}.linux.x86_64.tar.xz" -C "$work" "shellcheck-v$SHELLCHECK_VERSION/shellcheck"
    install -m 755 "$work/shellcheck-v$SHELLCHECK_VERSION/shellcheck" "$root/bin/shellcheck"
    ;;
  *) echo 'unknown provision group' >&2; exit 2;;
esac
if [[ "$1" == conformance ]]; then
  asset tweag/nickel "$NICKEL_VERSION" nickel-x86_64-linux "$NICKEL_SHA256"
  install -m 755 "$work/nickel-x86_64-linux" "$root/bin/nickel"
  asset oven-sh/bun "bun-v$BUN_VERSION" bun-linux-x64.zip "$BUN_SHA256"
  unzip -q "$work/bun-linux-x64.zip" -d "$work"
  install -m 755 "$work/bun-linux-x64/bun" "$root/bin/bun"
fi
if [[ -n ${GITHUB_PATH:-} ]]; then printf '%s\n' "$root/bin" >> "$GITHUB_PATH"; fi
printf 'Verified tools installed in %s\n' "$root/bin"
