# SPDX-License-Identifier: MPL-2.0
# Only a real protected environment can authorize the publication job.
.can_admins_bypass == false and
(.protection_rules | type == "array") and
any(.protection_rules[];
  .type == "required_reviewers" and .prevent_self_review == true and
  (.reviewers | type == "array" and length > 0))
