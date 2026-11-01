#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# publish-proposal.sh — commit a prepared standards proposal, push its branch and
# open exactly one pull request for it.
#
# WHY this is a script and not five lines of YAML: the guarantees are checks.
# It refuses to publish unless
#
#   * the working tree changes are confined to .ci/ (a proposal may move the
#     pin, the manifest, the revision log and add a receipt — nothing else);
#   * a receipt exists for the candidate revision and is verdict "accepted";
#   * the branch it pushes is the one named after the candidate revision, so a
#     rerun updates the same pull request instead of opening a second one.
#
# It also records, in the pull request body, which credential pushed it and
# whether the commit is signed. If the estate signing key is not provisioned the
# body says so; it does not pretend otherwise.
#
# USAGE
#   publish-proposal.sh HEAD_SHA [--previous SHA] [--dry-run] [--root DIR]
#
#   HEAD_SHA     the candidate standards revision (40-hex)
#   --previous   the revision being replaced (default: the newest record in
#                .ci/standards-revisions.tsv, which propose.sh just wrote)
#   --dry-run    print the branch, message and body; push nothing
#   --root DIR   repository root (default: this checkout)
#
# ENVIRONMENT
#   GH_TOKEN                token for gh (push and pull request)
#   ESTATE_TOKEN            set when the estate mechanism supplied the token
#   STANDARDS_SIGNING_KEY   base64 OpenPGP private key, when provisioned
#   STANDARDS_SIGNING_KEY_ID  its key id
#
# EXIT  0 published (or printed, with --dry-run); 2 cannot publish.
set -euo pipefail
IS_PROGRAM=publish-proposal.sh
here="$(cd -- "${0%/*}" && pwd)"
# shellcheck source=scripts/standards/lib.sh
. "$here/lib.sh"

usage() {
  sed -n '/^# USAGE/,/^# EXIT/p' "$0" | sed 's/^# \{0,1\}//'
}

root="$(ijson_standards_root)"
head=""
previous=""
dry_run=0
while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="${2:?--root needs a directory}"; shift 2 ;;
    --previous) previous="${2:?--previous needs a commit}"; shift 2 ;;
    --dry-run) dry_run=1; shift ;;
    -h | --help) usage; exit "$IS_EXIT_OK" ;;
    --) shift; break ;;
    -*) ijson_die "$IS_EXIT_CANNOT" "unknown option $1" ;;
    *)
      [ -z "$head" ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"
      head="$1"
      shift
      ;;
  esac
done
[ "$#" -eq 0 ] || ijson_die "$IS_EXIT_CANNOT" "unexpected argument: $1"
ijson_is_revision "$head" || ijson_die "$IS_EXIT_CANNOT" "HEAD_SHA must be a full 40-hex commit, got '$head'"

root="$(cd -- "$root" && pwd)"
pin="$(ijson_pin "$root" STANDARDS_REV)"
[ "$pin" = "$head" ] || ijson_die "$IS_EXIT_CANNOT" "the pin is $pin, not the candidate $head; run propose.sh first"

if [ -z "$previous" ]; then
  log="$root/.ci/standards-revisions.tsv"
  if [ -f "$log" ]; then
    previous="$(awk -F '\t' -v want="$head" 'NF >= 5 && $2 == want { was = $3 } END { print was }' "$log")"
  fi
fi
if [ -z "$previous" ]; then
  previous="unknown"
else
  ijson_is_revision "$previous" ||
    ijson_die "$IS_EXIT_WRONG_INPUT" "the revision log records an unusable previous revision: $previous"
fi

ijson_require git "publishing a proposal needs git"

receipts=("$root"/.ci/receipts/*"$head"*.json)
receipt="${receipts[0]}"
[ -f "$receipt" ] || ijson_die "$IS_EXIT_CANNOT" "no receipt for $head under .ci/receipts/; run verify.sh first"
if ! grep -q '"verdict":"accepted"' "$receipt"; then
  ijson_die "$IS_EXIT_CANNOT" "receipt $receipt is not an accepted verdict"
fi

if [ -n "${ESTATE_TOKEN:-}" ]; then
  credential="estate-app token"
else
  credential="workflow token (STANDARDS_UPDATE_TOKEN is not provisioned)"
fi

signed="no"
if [ -n "${STANDARDS_SIGNING_KEY:-}" ] && [ -n "${STANDARDS_SIGNING_KEY_ID:-}" ]; then
  gnupg="$(mktemp -d)"
  trap 'rm -rf -- "$gnupg"' EXIT
  export GNUPGHOME="$gnupg"
  chmod 700 "$gnupg"
  printf '%s' "$STANDARDS_SIGNING_KEY" | base64 -d | gpg --batch --import >/dev/null 2>&1 ||
    ijson_die "$IS_EXIT_CANNOT" "cannot import STANDARDS_SIGNING_KEY"
  git -C "$root" config commit.gpgsign true
  git -C "$root" config user.signingkey "$STANDARDS_SIGNING_KEY_ID"
  signed="yes"
fi

# last_line TEXT — the one line of a gh failure worth putting in a message.
last_line() {
  printf '%s\n' "$1" | grep -v '^$' | tail -n 1 | cut -c1-500
}

branch="standards/proposal-${head:0:12}"
short="${head:0:12}"
git -C "$root" checkout -B "$branch" >/dev/null 2>&1 ||
  ijson_die "$IS_EXIT_CANNOT" "cannot create branch $branch"
git -C "$root" add -- .ci >/dev/null

staged="$(git -C "$root" diff --cached --name-only)"
[ -n "$staged" ] || ijson_die "$IS_EXIT_CANNOT" "nothing staged; is .ci/ already at $short?"
outside="$(printf '%s\n' "$staged" | grep -v '^\.ci/' || true)"
if [ -n "$outside" ]; then
  ijson_die "$IS_EXIT_CANNOT" "a proposal may only change .ci/, but these are staged: $outside"
fi

body="$(mktemp)"
{
  printf 'Proposed by `.github/workflows/standards-update.yml`.\n\n'
  printf '* standards revision: `%s`\n' "$head"
  printf '* previous revision: `%s`\n' "$previous"
  printf '* pushed with: %s\n' "$credential"
  printf '* commit signed: %s\n' "$signed"
  printf '* receipt: `%s`\n\n' "${receipt#"$root"/}"
  printf '## What changed in standards\n\n```\n'
  cat "$root/target/ci/proposal.txt" 2>/dev/null || printf '(proposal output unavailable)\n'
  printf '```\n\n## Receipt\n\n```json\n'
  cat "$receipt"
  printf '```\n\n'
  printf 'Merging this pull request moves the pin. Nothing follows `main` at run time:\n'
  printf 'the pin only moves when this is merged, and the receipt records the\n'
  printf 'standards revision, the toolchain and the binary the checks ran against.\n'
} >"$body"

message="standards: propose $short (was ${previous:0:12})"

if [ "$dry_run" -eq 1 ]; then
  printf 'publish: dry run\n  branch: %s\n  message: %s\n  staged:\n' "$branch" "$message"
  printf '%s\n' "$staged" | sed 's/^/    /'
  printf '  body:\n'
  sed 's/^/    /' "$body"
  exit "$IS_EXIT_OK"
fi

# gh is only needed to publish; a --dry-run review works without it.
ijson_require gh "publishing a proposal needs the GitHub CLI"

# The workflow checks out with persist-credentials: false, so the remote has no
# stored credential. gh's credential helper reads GH_TOKEN from the environment;
# setting it up writes no secret to disk.
gh auth setup-git >/dev/null 2>&1 ||
  printf 'publish: gh could not install a credential helper; relying on the environment\n' >&2

git -C "$root" -c user.name="hyperpolymath-standards-bot" \
  -c user.email="standards-bot@users.noreply.github.com" \
  commit -m "$message" >/dev/null ||
  ijson_die "$IS_EXIT_CANNOT" "cannot commit the proposal"
git -C "$root" push --force-with-lease origin "$branch" >/dev/null 2>&1 ||
  ijson_die "$IS_EXIT_CANNOT" "cannot push $branch"

repo="${GITHUB_REPOSITORY:-hyperpolymath/ijson-jcs}"
existing="$(gh pr list --repo "$repo" --head "$branch" --state open --json number \
  --jq 'if length == 0 then "" else (.[0].number | tostring) end')" ||
  ijson_die "$IS_EXIT_CANNOT" "cannot list pull requests in $repo"
if [ -n "$existing" ]; then
  if ! answer="$(gh pr edit "$existing" --repo "$repo" --body-file "$body" 2>&1)"; then
    # gh's own diagnosis is the actionable part: a token that may not open pull
    # requests is the estate mechanism being unprovisioned, and it says so.
    ijson_die "$IS_EXIT_CANNOT" "cannot update pull request $existing: $(last_line "$answer")"
  fi
  printf 'publish: updated pull request %s on %s\n' "$existing" "$branch"
else
  # The base is the repository's default branch, not the branch this job is
  # running on: a proposal from a feature branch is still a proposal against
  # main, and asking gh to infer it gets that wrong outside a push to main.
  base="$(gh api "repos/$repo" --jq .default_branch 2>/dev/null || printf 'main')"
  [ -n "$base" ] || base=main
  if ! answer="$(gh pr create --repo "$repo" --head "$branch" --base "$base" \
    --title "standards: propose $short" --body-file "$body" 2>&1)"; then
    ijson_die "$IS_EXIT_CANNOT" "cannot open a pull request in $repo: $(last_line "$answer")"
  fi
  printf 'publish: opened %s\n' "$answer"
fi
