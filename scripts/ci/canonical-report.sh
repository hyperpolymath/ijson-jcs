#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Render a generated report with the candidate canonicalizer and one final LF.
set -euo pipefail
input=${1:?raw input required} output=${2:?output required}
binary=${IJSON_JCS:-$PWD/target/debug/ijson-jcs}
test -s "$input"
"$binary" canon < "$input" > "$output.pending"
printf '\n' >> "$output.pending"
mv -- "$output.pending" "$output"
