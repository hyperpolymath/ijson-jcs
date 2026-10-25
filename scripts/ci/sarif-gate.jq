# SPDX-License-Identifier: MPL-2.0
# CodeQL emits query packs under tool.extensions; older reports use driver.rules.
def rules: (.tool.driver.rules // []) + [.tool.extensions[]?.rules[]?];
.version == "2.1.0" and (.runs | type == "array" and length > 0) and
all(.runs[];
  rules as $definitions |
  .tool.driver.name == "CodeQL" and
  ($definitions | length > 0) and
  ($definitions | map(.id) | unique | length) == ($definitions | length) and
  (.invocations | type == "array" and length > 0) and
  all(.invocations[]; .executionSuccessful == true) and
  (.results | type == "array") and
  ($definitions | map({key:.id,value:.}) | from_entries) as $rules |
  all(.results[];
    $rules[.ruleId // .rule.id] as $rule |
    if $rule == null then error("result has no matching rule metadata")
    else
      ($rule.properties."security-severity" // "0" | tonumber) as $severity |
      ((.level // $rule.defaultConfiguration.level // "warning") != "error") and
      ($severity | type == "number" and (isnan | not) and (isinfinite | not)) and
      $severity >= 0 and $severity < 7
    end))
