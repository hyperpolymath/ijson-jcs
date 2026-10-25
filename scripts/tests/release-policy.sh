#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
# Explicit synthetic authorization-policy controls, NEVER live permissions.
jq -n '{can_admins_bypass:false,protection_rules:[{type:"required_reviewers",prevent_self_review:true,reviewers:[{type:"User",reviewer:{id:1}}]}]}' > "$work/clean"
jq -e -f scripts/ci/release-policy.jq "$work/clean" >/dev/null
for fault in '.can_admins_bypass = true' 'del(.can_admins_bypass)' \
  '.protection_rules = []' 'del(.protection_rules)' \
  '.protection_rules[0].prevent_self_review = false' \
  '.protection_rules[0].reviewers = []' '.protection_rules[0].type = "wait_timer"'; do
  jq "$fault" "$work/clean" > "$work/fault"
  if jq -e -f scripts/ci/release-policy.jq "$work/fault" >/dev/null 2>&1; then
    echo "Unsafe release policy was accepted: $fault" >&2; exit 1
  fi
done
status=0
GITHUB_REF_TYPE=branch GITHUB_REF_NAME=main bash scripts/ci/release-check.sh > "$work/log" 2>&1 || status=$?
(( status == 2 )) || { cat "$work/log"; echo 'branch release was not refused' >&2; exit 1; }
echo 'Release policy clean/firing controls passed; no live authorization is claimed'
