#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Trusted main only, isolated from candidate script execution and write tokens.
set -euo pipefail
[[ ${GITHUB_REF:-} == refs/heads/main ]] || { echo 'only trusted main publishes proposals' >&2; exit 2; }
receipt=${1:?receipt required}
jq -e '.schema_version == 1 and .status == "tested-proposal" and .provenance == "Unproven" and
  (.candidate | test("^[0-9a-f]{40}$")) and (.previous | test("^[0-9a-f]{40}$")) and
  (.implementation | test("^[0-9a-f]{40}$"))' "$receipt" >/dev/null
candidate=$(jq -r .candidate "$receipt")
previous=$(jq -r .previous "$receipt")
title="Standards update: $candidate"
existing=$(gh issue list --repo "$GITHUB_REPOSITORY" --state open --search 'in:title "Standards update:"' --json number,title --jq ".[] | select(.title == \"$title\") | .number")
[[ -z "$existing" ]] || { echo "Existing proposal: #$existing"; exit 0; }
body=$(mktemp); trap 'rm -f "$body"' EXIT
cat > "$body" <<BODY
## Tested immutable standards proposal

Candidate: \`$candidate\`; previously accepted: \`$previous\`.

The read-only candidate job ran the library/CLI tests, both accepted and candidate
standards JSON-policy fixture suites, independent failure controls and framing tests.
This is empirical test evidence, not proof or owner affirmation.

Review the exact input hashes and apply the generated \`standards.patch\` through
the authorised signed-commit mechanism. Re-run all required CI on the resulting
commit. No automatic pin change, merge, signature, kyq update or publication has
occurred. Companion work remains tracked in #7.

Receipt and patch: https://github.com/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID
Policy diff: https://github.com/metadatastician/standards/compare/$previous...$candidate
BODY
gh issue create --repo "$GITHUB_REPOSITORY" --title "$title" --body-file "$body"
