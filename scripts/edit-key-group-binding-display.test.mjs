import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const editKeySource = await readFile("src/features/key-pool/EditKeyPage.tsx", "utf8");
const keyPoolSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const addKeySource = await readFile("src/features/key-pool/AddKeyPage.tsx", "utf8");

assert.ok(
  editKeySource.includes("setForm(formFromItem(item, nextGroupOptions))"),
  "edit-key form initialization should resolve the current group selection from loaded group options",
);

assert.ok(
  editKeySource.includes("findMatchingGroupOption") &&
    /function groupBindingValueFromItem\([\s\S]*groupIdHash: item\.groupIdHash[\s\S]*groupName: item\.groupName \?\? ""[\s\S]*option\?\.groupBindingId/.test(editKeySource),
  "edit-key page should preselect a current group binding by group id hash/name when the key lacks groupBindingId",
);

for (const [sourceName, source] of [
  ["edit-key page", editKeySource],
  ["legacy key-pool dialog", keyPoolSource],
]) {
  assert.ok(
    source.includes("StationGroupOptionLabel"),
    `${sourceName} should use the shared station-style group option chip without backend source suffixes`,
  );
  const groupOptionLabelBody = source.match(/function groupOptionLabel\([\s\S]*?\n}/)?.[0] ?? "";
  assert.ok(
    groupOptionLabelBody && !groupOptionLabelBody.includes("rateSource"),
    `${sourceName} group option label should not append raw rateSource values like sub2api_groups_rates`,
  );
}

for (const [sourceName, source] of [["add-key page", addKeySource]]) {
  assert.ok(
    source.includes("formatStationGroupOptionLabel(option)"),
    `${sourceName} should use the shared group option label without backend source suffixes`,
  );
  const groupOptionLabelBody = source.match(/function groupOptionLabel\([\s\S]*?\n}/)?.[0] ?? "";
  assert.ok(
    groupOptionLabelBody && !groupOptionLabelBody.includes("rateSource"),
    `${sourceName} group option label should not append raw rateSource values like sub2api_groups_rates`,
  );
}
