# ijson-jcs

Rust library and CLI for strict I-JSON-oriented parsing (RFC 7493) and
JCS canonicalization (RFC 8785). This is the Hyperpolymath estate's JSON
implementation. Estate YAML uses **KYAML**, not JSON disguised as YAML.

```sh
cargo install --path . --locked
printf '{"b":2,"a":1}' | ijson-jcs canon
ijson-jcs check input.json
ijson-jcs fix -- input.json
```

See [README.adoc](README.adoc) for the API, exact number/Unicode profile,
filesystem limits, and the difference between JCS bytes and estate file framing.
See [CI and verification](docs/CI.adoc) for reproducible checks, evidence,
benchmark bootstrapping, and the required owner-side protection setup.
[Upstream work](docs/UPSTREAM-PROMPTS.adoc) records cross-repository blockers.

Code: MPL-2.0. Documentation declarations are preserved; new AsciiDoc guides
are CC-BY-SA-4.0. Tests and scanners provide empirical evidence, not formal proof.
