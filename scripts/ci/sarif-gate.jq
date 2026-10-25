# SPDX-License-Identifier: MPL-2.0
# Consume CodeQL's actual rules/results, not just its process exit status.
.version == "2.1.0" and (.runs | type == "array" and length > 0) and
all(.runs[];
  .tool.driver.name == "CodeQL" and
  (.tool.driver.rules | type == "array" and length > 0) and
  (.results | type == "array") and
  (.tool.driver.rules | map({key:.id,value:.}) | from_entries) as $rules |
  all(.results[];
    $rules[.ruleId] as $rule |
    if $rule == null then error("result has no matching rule metadata")
    else
      ($rule.properties."security-severity" // "0" | tonumber) as $severity |
      ((.level // $rule.defaultConfiguration.level // "warning") != "error") and
      ($severity | type == "number" and (isnan | not) and (isinfinite | not)) and
      $severity >= 0 and $severity < 7
    end))
