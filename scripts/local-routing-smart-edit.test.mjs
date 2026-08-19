import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const editor = readFileSync("src/features/routing/LocalRoutingSettingsEditor.tsx", "utf8");
const editTab = readFileSync("src/features/routing/LocalRoutingEditTab.tsx", "utf8");

assert.match(editTab, /LocalRoutingSettingsEditor/);
assert.match(editor, /loadRoutingPolicy/);
assert.match(editor, /applyRoutingPolicyDocument/);
assert.match(editor, /baseRevision/);
for (const field of ["reliabilityWeight", "responsivenessWeight", "costWeight", "preferenceWeight"]) {
  assert.match(editor, new RegExp(field));
}
assert.match(editor, /total !== 10_000/);
assert.doesNotMatch(editor, /schedulerAdvancedSettings|getSettings|updateSettings|LocalRoutingSettingsFields|localRoutingSettingsForm/);
console.log("routing policy editor contract ok");
