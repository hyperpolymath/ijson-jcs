K9!
# SPDX-License-Identifier: MPL-2.0
# Kennel data only. Conformance is L0-L2, not a signature/execution licence.
{
  pedigree = {
    schema_version = "1.0.0",
    component_type = "project-metadata",
    security = {
      leash = 'Kennel,
      trust_level = "data-only",
      allow_network = false,
      allow_filesystem_write = false,
      allow_subprocess = false,
      capabilities = [],
    },
    metadata = {
      name = "ijson-jcs-ci",
      version = "1.0.0",
      description = "CI obligations and evidence routing; not executable deployment authority.",
    },
    side_effects = [],
  },
  config = {
    project = "ijson-jcs",
    required_context = "Required CI",
    evidence_directory = "target/ci",
    normative_inputs = ".ci/standards.sha256",
    proof_status = "Unproven: empirical Rust and shell tests",
    promotion = "reviewed immutable revision; never bypass a gate",
  },
}
