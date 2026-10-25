<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Security policy

Report suspected vulnerabilities privately through GitHub's **Report a
vulnerability** facility when enabled, or to **j.d.a.jewell@open.ac.uk**.
Do not publish secret material or exploit data in public issues.

The unreleased 0.1.x line is supported on a best-effort basis. Include the exact
commit, toolchain, input bytes (redacted if sensitive), expected/actual result,
and a minimal reproducer. Canonical byte discrepancies matter for signatures.

The CLI bounds input size (64 MiB by default, configurable with `--max-bytes`).
Text and built values have nesting limits. `fix` uses same-directory atomic
replacement and rejects symlinks, Unix hard links and read-only files. It is not
an adversarial filesystem sandbox or a concurrent-writer lock; extended
attributes/ACLs/ownership are not promised preserved. Keep backups and coordinate
writers. Invalid inputs are never rewritten.

Never re-canonicalize archived signed bytes without verifying the original
signature and explicitly versioning the migration. See `docs/CI.adoc` for
supply-chain checks and `docs/UPSTREAM-PROMPTS.adoc` for unresolved boundaries.
