// SPDX-License-Identifier: MPL-2.0
// Test oracle only: run with pinned Bun (JavaScriptCore), not a JSON replacement.
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const lines = readFileSync(process.argv[2], "utf8").trimEnd().split("\n");
assert(lines.length > 19000, "missing/truncated binary64 corpus");
const buffer = new ArrayBuffer(8);
const view = new DataView(buffer);
for (const line of lines) {
  const [bits, canonical] = JSON.parse(line);
  assert.match(bits, /^[0-9a-f]{16}$/);
  view.setBigUint64(0, BigInt(`0x${bits}`));
  const value = view.getFloat64(0);
  assert(Number.isFinite(value));
  assert.equal(canonical, JSON.stringify(value), `binary64 ${bits}`);
}
console.log(`${lines.length} binary64 values agree with ECMAScript JSON.stringify`);
