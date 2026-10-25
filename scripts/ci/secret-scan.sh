#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# No broad fixture/comment/AsciiDoc allowlist. Both populations must be scanned.
set -euo pipefail
source .ci/pins.sh
[[ "$(uname -s)/$(uname -m)" == Linux/x86_64 ]] || { echo 'supported on Linux/x86_64 CI only' >&2; exit 2; }
[[ "$(git rev-parse --is-shallow-repository)" == false ]] || { echo 'full history required' >&2; exit 2; }
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
asset=gitleaks_${GITLEAKS_VERSION}_linux_x64.tar.gz
gh release download "v$GITLEAKS_VERSION" --repo gitleaks/gitleaks --pattern "$asset" --dir "$work"
printf '%s  %s\n' "$GITLEAKS_SHA256" "$work/$asset" | sha256sum --check --status
tar -xzf "$work/$asset" -C "$work" gitleaks
binary=$work/gitleaks
"$binary" version
mkdir -p "$work/clean" "$work/fault" target/ci
printf 'No credentials in this control.\n' > "$work/clean/control.txt"
# A synthetic, never-valid credential-shaped string, generated only at runtime.
# The firing test must fail even though it is in a comment in an AsciiDoc file.
# Gitleaks intentionally allowlists the literal alphabet example. Derive a
# reproducible non-allowlisted dummy instead; never change the scanner rules.
dummy=$(printf '%s' 'ijson-jcs synthetic detector control' | sha256sum)
printf '# token = "%s%s"\n' 'ghp_' "${dummy:0:36}" > "$work/fault/control.adoc"
"$binary" dir "$work/clean" --redact --no-banner > "$work/clean.log" 2>&1
status=0
"$binary" dir "$work/fault" --redact --no-banner > "$work/fault.log" 2>&1 || status=$?
(( status == 1 )) || { cat "$work/fault.log"; echo 'secret-detector firing control did not fire' >&2; exit 2; }
status=0
"$binary" dir . --redact --no-banner --report-format json --report-path target/ci/secrets-tree.raw || status=$?
"$binary" git . --log-opts='--all' --redact --no-banner --report-format json --report-path target/ci/secrets-history.raw || status=$?
cargo build --locked --bin ijson-jcs
for report in tree history; do
  [[ -s "target/ci/secrets-$report.raw" ]] || { echo 'missing secret-scan report' >&2; exit 2; }
  bash scripts/ci/canonical-report.sh "target/ci/secrets-$report.raw" "target/ci/secrets-$report.json"
  jq -e '. == null or (type == "array" and length == 0)' "target/ci/secrets-$report.json" >/dev/null
done
git rev-list --all --count > target/ci/secret-commit-count.txt
exit "$status"
