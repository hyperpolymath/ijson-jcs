#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Exact normative inputs and upstream failure controls against THIS binary.
set -euo pipefail
source .ci/pins.sh
revision=${1:-$STANDARDS_REV}
standards=$(bash scripts/ci/fetch-source.sh "standards-$revision" "$STANDARDS_REPOSITORY" "$revision")
if [[ "$revision" == "$STANDARDS_REV" ]]; then
  (cd "$standards" && sha256sum --check "$OLDPWD/.ci/standards.sha256")
else
  # A candidate must contain every presently required normative input. Hash
  # differences are the proposal, not silent replacement of the accepted pin.
  while read -r _ path; do test -s "$standards/$path"; done < .ci/standards.sha256
fi
export IJSON_JCS=${IJSON_JCS:-$PWD/target/debug/ijson-jcs}
test -x "$IJSON_JCS"
bash "$standards/scripts/tests/check-ijson-jcs-test.sh"
bash scripts/tests/estate-json.sh "$IJSON_JCS"
printf 'Standards fixture suite executed at %s against %s\n' "$revision" "$IJSON_JCS"
