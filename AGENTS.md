<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Working on ijson-jcs

Read `README.md`, `docs/CI.adoc`, and `verification/suites.toml` before changes.

- This repository is the estate JSON implementation. Do not substitute a
  similarly named crate, `jq`, or a stub kyq validator for its canonicalizer.
- Author YAML content as KYAML. GitHub still requires `.yml`/`.yaml` filenames.
  Use pinned standards `kyaml-format.sh` and `yq`, never regex/Python YAML gates.
- Keep new automation in Rust or small Bash scripts. Bun is the independent
  ECMAScript test oracle and the runtime for the upstream DEED validator.
- A missing tool, missing fixture, empty scan, skipped required job or malformed
  report is not a pass. Test harnesses need clean and firing controls.
- Tests here are empirical (Unproven in proven-tests' taxonomy). No formal proof
  or owner affirmation may be invented. Preserve historical affirmations.
- Never disable security checks, drop required contexts, admin-merge, auto-merge
  a work-in-progress, or resolve a review thread without addressing its finding.
- Preserve existing licence/SPDX declarations. Code is MPL-2.0; new prose is
  CC-BY-SA-4.0. Added licence texts supply existing declarations, not relicensing.
- Keep generated evidence, tool checkouts and fuzz artifacts out of Git.
