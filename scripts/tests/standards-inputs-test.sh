#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# SPDX-FileCopyrightText: 2026 Hyperpolymath Estate
#
# standards-inputs-test.sh — prove the standards pipeline can fail.
#
# It builds a synthetic standards tree from this repository's own contract, and
# a scratch repository root, then drives scripts/standards/* through the cases a
# gate has to distinguish:
#
#   * a matching tree verifies and its contract holds (the clean control, so a
#     checker that returns 0 for everything cannot pass this test);
#   * a tampered file, a missing file and a manifest path that escapes the tree
#     are exit 3, not 0;
#   * a policy sentence that moved is exit 3 and is *named* as an incompatible
#     policy change, not merely a hash mismatch;
#   * drift, no-drift, an unparseable head and a missing git are 1, 0, 2 and 2;
#   * a proposal for the revision already pinned is 1, a dry run writes
#     nothing, and a real proposal rewrites the pin, the manifest and the log;
#   * verify.sh and kyq-vectors.sh without a binary are exit 2.
#
# No network, no Rust toolchain, no binary: this tests the pipeline's own
# fail-closed behaviour, which is the part that quietly rots.
#
# EXIT  0 every control behaved; 1 a control did not; 2 the test could not run.
set -euo pipefail

# Synthetic repositories must not invoke a developer's signing agent, hooks or
# credential helpers. This subprocess only creates disposable fixture commits;
# the production publisher's signing requirements are unchanged.
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_CONFIG_NOSYSTEM=1
unset GIT_CONFIG GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT

repo_root="$(cd -- "${0%/*}/../.." && pwd)"
scripts="$repo_root/scripts/standards"
contract="$repo_root/.ci/standards-contract.tsv"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

# Older Git versions do not recognise GIT_CONFIG_GLOBAL. Give them empty
# traditional config locations too, and never copy caller-provided init hooks.
mkdir -p "$work/home" "$work/xdg" "$work/git-template"
export HOME="$work/home" XDG_CONFIG_HOME="$work/xdg"
export GIT_TEMPLATE_DIR="$work/git-template"

pass=0
fail=0

# expect WANT LABEL COMMAND... — run COMMAND and compare its exit code.
expect() {
  local want="$1" label="$2" got=0
  shift 2
  "$@" >"$work/out" 2>&1 || got=$?
  if [ "$got" = "$want" ]; then
    pass=$((pass + 1))
    printf '  ok    %s\n' "$label"
  else
    fail=$((fail + 1))
    printf '  FAIL  %s (wanted exit %s, got %s)\n' "$label" "$want" "$got" >&2
    sed 's/^/        /' "$work/out" >&2
  fi
}

# said NEEDLE LABEL — assert the last command's output mentions NEEDLE.
said() {
  if grep -qF -- "$1" "$work/out"; then
    pass=$((pass + 1))
    printf '  ok    %s\n' "$2"
  else
    fail=$((fail + 1))
    printf '  FAIL  %s (output never said %s)\n' "$2" "$1" >&2
    sed 's/^/        /' "$work/out" >&2
  fi
}

# same LABEL — a direct assertion, counted like every other control.
same() {
  if [ "$1" = "$2" ]; then
    pass=$((pass + 1))
    printf '  ok    %s\n' "$3"
  else
    fail=$((fail + 1))
    printf '  FAIL  %s (%s != %s)\n' "$3" "$1" "$2" >&2
  fi
}

for tool in sha256sum bash awk sed grep find mktemp; do
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'standards-inputs-test: %s is not on PATH\n' "$tool" >&2
    exit 2
  }
done
[ -f "$contract" ] || {
  printf 'standards-inputs-test: no contract at %s\n' "$contract" >&2
  exit 2
}

# ---------------------------------------------------------------------------
# A scratch repository root, and synthetic standards trees built from the real
# contract: `old` is what the scratch pin records, `new` is a compatible
# revision of it, `moved` has one policy sentence reworded.
# ---------------------------------------------------------------------------
scratch="$work/repo"
mkdir -p "$scratch/.ci"
cp -- "$contract" "$scratch/.ci/standards-contract.tsv"
printf 'date\trevision\tprevious\tmanifest_sha256\tsource\n' >"$scratch/.ci/standards-revisions.tsv"
cat >"$scratch/.ci/pins.sh" <<'PINS'
# scratch pin file for scripts/tests/standards-inputs-test.sh
readonly STANDARDS_REPOSITORY='metadatastician/standards'
readonly STANDARDS_URL_BASE="${STANDARDS_URL_BASE:-https://codeload.github.com}"
readonly STANDARDS_REV='1111111111111111111111111111111111111111'
PINS

# make_tree DIR VARIANT [SKIP_LITERAL] — one line per contract assertion, so the
# tree satisfies the contract unless SKIP_LITERAL is removed from it.
make_tree() {
  local tree="$1" variant="$2" skip="${3:-}"
  rm -rf -- "$tree"
  mkdir -p -- "$tree"
  while IFS=$'\t' read -r role path literal; do
    if [ -n "$skip" ] && [ "$literal" = "$skip" ]; then
      continue
    fi
    mkdir -p -- "$tree/${path%/*}"
    printf '%s: %s\n' "$role" "$literal" >>"$tree/$path"
  done < <(awk -F'\t' '!/^#/ && NF==3' "$contract")
  mkdir -p -- "$tree/extra"
  printf 'pinned, not part of the contract (%s)\n' "$variant" >"$tree/extra/fixture.txt"
}

old="$work/old"
new="$work/new"
moved="$work/moved"
skip_literal="$(awk -F'\t' '$1=="rule-j-1" {print $3; exit}' "$contract")"
make_tree "$old" old
make_tree "$new" new
make_tree "$moved" moved "$skip_literal"

# The manifest the scratch pin records: the contract paths plus one extra file.
(
  cd "$old"
  find . -type f | sed 's|^\./||' | sort | xargs sha256sum
) >"$scratch/.ci/standards.sha256"

echo 'contract.sh'
expect 0 'the clean control: the synthetic tree satisfies the contract' \
  bash "$scripts/contract.sh" --root "$scratch" --tree "$old"
said 'assertion(s) over' 'it reports how many assertions held'
expect 3 'a moved policy sentence is an incompatible policy change' \
  bash "$scripts/contract.sh" --root "$scratch" --tree "$moved"
said 'INCOMPATIBLE POLICY CHANGE' 'it names the change as incompatible'
said "$skip_literal" 'it quotes the sentence that moved'
mkdir -p "$work/empty-tree"
expect 3 'a tree with no pinned files fails the contract' \
  bash "$scripts/contract.sh" --root "$scratch" --tree "$work/empty-tree"

echo 'fetch.sh'
expect 0 'a recorded tree verifies against its manifest' \
  bash "$scripts/fetch.sh" --root "$scratch" --source "$old"
said 'verified' 'it says how many files it verified'

tampered="$work/tampered"
cp -a "$old" "$tampered"
printf 'drift\n' >>"$tampered/scripts/check-ijson-jcs.sh"
expect 3 'a tampered file is detected' \
  bash "$scripts/fetch.sh" --root "$scratch" --source "$tampered"
said 'pinned file changed' 'it names the hash mismatch'

missing="$work/missing"
cp -a "$old" "$missing"
rm -f -- "$missing/scripts/check-ijson-jcs.sh"
expect 3 'a missing pinned file is detected' \
  bash "$scripts/fetch.sh" --root "$scratch" --source "$missing"
said 'pinned file missing' 'it names the missing file'

escaping="$work/escaping"
cp -a "$scratch" "$escaping"
printf '%s  ../outside\n' \
  0000000000000000000000000000000000000000000000000000000000000000 \
  >>"$escaping/.ci/standards.sha256"
expect 3 'a manifest path that escapes the tree is refused' \
  bash "$scripts/fetch.sh" --root "$escaping" --source "$old"
said 'escapes the tree' 'it says why'

expect 2 'a revision that is not a full commit is refused' \
  bash "$scripts/fetch.sh" --root "$scratch" --rev deadbeef --source "$old"
expect 2 'no network and no cache is exit 2, not a pass' \
  bash "$scripts/fetch.sh" --root "$scratch" --rev 2222222222222222222222222222222222222222 --no-download

echo 'detect.sh'
expect 0 'no drift when the head is the pin' \
  bash "$scripts/detect.sh" --root "$scratch" --head 1111111111111111111111111111111111111111
said 'drift=false' 'it reports no drift'
expect 1 'drift when the head is another commit' \
  bash "$scripts/detect.sh" --root "$scratch" --head 2222222222222222222222222222222222222222
said 'drift=true' 'it reports drift'
expect 1 'json output is available' \
  bash "$scripts/detect.sh" --root "$scratch" --head 2222222222222222222222222222222222222222 --format json
said '"drift":true' 'the json says drift'
expect 2 'an unparseable head is exit 2' \
  bash "$scripts/detect.sh" --root "$scratch" --head not-a-commit
if command -v git >/dev/null 2>&1; then
  expect 2 'a missing git is exit 2, never "no drift"' \
    env PATH=/nonexistent-standards-test /bin/bash "$scripts/detect.sh" --root "$scratch"
fi

echo 'propose.sh'
expect 1 'proposing the revision already pinned is a no-op' \
  bash "$scripts/propose.sh" --root "$scratch" --rev 1111111111111111111111111111111111111111
said 'already pinned' 'it says so'
expect 3 'a candidate that drops a pinned file is refused' \
  bash "$scripts/propose.sh" --root "$scratch" --rev 2222222222222222222222222222222222222222 --source "$missing"
said 'no longer carries a pinned input' 'it says why'
expect 3 'a candidate whose policy moved is refused' \
  bash "$scripts/propose.sh" --root "$scratch" --rev 2222222222222222222222222222222222222222 --source "$moved"
said 'INCOMPATIBLE POLICY CHANGE' 'it names the incompatible change'

before_pin="$(cat "$scratch/.ci/pins.sh")"
before_manifest="$(cat "$scratch/.ci/standards.sha256")"
expect 0 'a dry run reports a compatible proposal' \
  bash "$scripts/propose.sh" --root "$scratch" --rev 2222222222222222222222222222222222222222 --source "$new" --dry-run
said 'CHANGED extra/fixture.txt' 'it names the recorded hash that moves'
said '--dry-run, nothing written' 'it says it wrote nothing'
same "$(cat "$scratch/.ci/pins.sh")" "$before_pin" 'the dry run left the pin alone'
same "$(cat "$scratch/.ci/standards.sha256")" "$before_manifest" 'the dry run left the manifest alone'

expect 0 'a compatible proposal is written' \
  bash "$scripts/propose.sh" --root "$scratch" --rev 2222222222222222222222222222222222222222 --source "$new"
said 'wrote .ci/pins.sh' 'it says what it wrote'
expect 0 'the new pin verifies against the new manifest' \
  bash "$scripts/fetch.sh" --root "$scratch" --source "$new"
expect 0 'the new pin still satisfies the contract' \
  bash "$scripts/contract.sh" --root "$scratch" --tree "$new"
same "$(grep -c "^readonly STANDARDS_REV='2222222222222222222222222222222222222222'$" "$scratch/.ci/pins.sh")" 1 'the pin names the candidate revision exactly once'
same "$(wc -l <"$scratch/.ci/standards-revisions.tsv" | tr -d ' ')" 2 'the proposal appended one revision record'

echo 'verify.sh and kyq-vectors.sh cannot run without a binary'
expect 2 'verify.sh with no binary is exit 2, not a pass' \
  bash "$scripts/verify.sh" --root "$scratch" --no-build --binary "$work/does-not-exist" --source "$new"
expect 2 'kyq-vectors.sh with no binary is exit 2' \
  bash "$scripts/kyq-vectors.sh" verify --root "$repo_root" --binary "$work/does-not-exist"

# A binary that reproduces nothing must not verify. This is the control that
# keeps "the vectors passed" from meaning "the script exited 0".
stub="$work/stub-binary"
printf '#!/usr/bin/env bash\nexit 0\n' >"$stub"
chmod +x "$stub"
kyq_copy="$work/kyq"
cp -a "$repo_root/interop/kyq" "$kyq_copy"
expect 1 'a binary that reproduces no vector is a finding' \
  bash "$scripts/kyq-vectors.sh" verify --dir "$kyq_copy" --binary "$stub"
said 'FAIL' 'it names the cases that differ'
same "$(sha256sum <"$kyq_copy/vectors.sha256")" \
  "$(sha256sum <"$repo_root/interop/kyq/vectors.sha256")" \
  'a failed verify leaves the manifest alone'

# Re-recording may not delete a curated refusal assertion to make itself pass.
mkdir -p "$kyq_copy/vectors/99-stub-curated"
printf 'canon\n' >"$kyq_copy/vectors/99-stub-curated/cmd"
printf '{"b":1,"a":2}\n' >"$kyq_copy/vectors/99-stub-curated/stdin"
printf 'INVALID stub\n' >"$kyq_copy/vectors/99-stub-curated/expect.stdout_prefix"
printf '1\n' >"$kyq_copy/vectors/99-stub-curated/expect.exit"
manifest_before="$(sha256sum <"$kyq_copy/vectors.sha256")"
expect 1 're-recording refuses to drop a curated assertion' \
  bash "$scripts/kyq-vectors.sh" write --dir "$kyq_copy" --binary "$stub"
said 'REFUSED' 'it names the vector it refused'
same "$(sha256sum <"$kyq_copy/vectors.sha256")" "$manifest_before" \
  'a refused re-record leaves the manifest alone'
same "$(cat "$kyq_copy/vectors/99-stub-curated/expect.stdout_prefix")" 'INVALID stub' \
  'the curated assertion is still there'

echo 'publish-proposal.sh'
if command -v git >/dev/null 2>&1; then
  (
    cd "$scratch" &&
      git init -q &&
      git config user.email "test@example.invalid" &&
      git config user.name "standards test" &&
      git add -A &&
      git commit -qm base
  )
  candidate=2222222222222222222222222222222222222222
  mkdir -p "$scratch/.ci/receipts"
  receipt="$scratch/.ci/receipts/2026-11-01T000000Z-$candidate.json"
  printf '{"verdict":"accepted"}\n' >"$receipt"

  expect 0 'a proposal dry run publishes nothing' \
    bash "$scripts/publish-proposal.sh" --root "$scratch" --dry-run "$candidate"
  said 'branch: standards/proposal-222222222222' 'it names the branch'
  said 'previous revision: `1111111111111111111111111111111111111111`' \
    'it took the previous revision from the revision log, not the new pin'

  mv -- "$receipt" "$work/hidden-receipt.json"
  expect 2 'a proposal without an accepted receipt is refused' \
    bash "$scripts/publish-proposal.sh" --root "$scratch" "$candidate"
  said 'no receipt for' 'it says which receipt is missing'
  mv -- "$work/hidden-receipt.json" "$receipt"

  printf 'not part of the proposal\n' >"$scratch/outside.txt"
  git -C "$scratch" add -- outside.txt
  expect 2 'a proposal may stage nothing outside .ci/' \
    bash "$scripts/publish-proposal.sh" --root "$scratch" "$candidate"
  said 'may only change .ci/' 'it names the rule'
  rm -f -- "$scratch/outside.txt"
  git -C "$scratch" reset -q
  # Commit the receipt so that .ci/ really is clean for the next control.
  git -C "$scratch" add -A
  git -C "$scratch" commit -qm receipt

  expect 2 'a proposal with no staged change is refused' \
    bash "$scripts/publish-proposal.sh" --root "$scratch" "$candidate"
  said 'nothing staged' 'it says there is nothing to publish'
fi

echo 'this repository'
real_pin="$(
  # shellcheck disable=SC1091
  . "$repo_root/.ci/pins.sh"
  printf '%s\n' "$STANDARDS_REV"
)"
expect 0 'the real pin reports no drift against itself' \
  bash "$scripts/detect.sh" --root "$repo_root" --head "$real_pin"
expect 0 'the real contract lists its assertions' \
  bash "$scripts/contract.sh" --root "$repo_root" --list

printf 'passed %d, failed %d\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
