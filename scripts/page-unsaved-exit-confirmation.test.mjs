import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pageContracts = [
  {
    file: "src/features/stations/AddProviderPage.tsx",
    exitHandler: "requestExit",
    snapshotHelper: "serializeProviderDraft",
  },
  {
    file: "src/features/key-pool/AddKeyPage.tsx",
    exitHandler: "requestExit",
    snapshotHelper: "serializeAddKeyForm",
  },
  {
    file: "src/features/key-pool/EditKeyPage.tsx",
    exitHandler: "requestExit",
    snapshotHelper: "serializeEditKeyForm",
  },
];

for (const contract of pageContracts) {
  const source = await readFile(contract.file, "utf8");

  assert.match(
    source,
    /ConfirmDialog/,
    `${contract.file} should render the shared in-app confirmation dialog`,
  );
  assert.match(
    source,
    new RegExp(`function ${contract.exitHandler}\\(`),
    `${contract.file} should route cancel/back through a shared exit handler`,
  );
  assert.match(
    source,
    new RegExp(`function ${contract.snapshotHelper}\\(`),
    `${contract.file} should compare the current draft against an initial snapshot`,
  );
  assert.match(
    source,
    /setDiscardConfirmOpen\(true\)/,
    `${contract.file} should open the discard confirmation when unsaved changes exist`,
  );
  assert.match(
    source,
    /confirmLabel="放弃修改"/,
    `${contract.file} should make the destructive discard action explicit`,
  );
}

const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");

assert.doesNotMatch(
  addProviderSource,
  /return JSON\.stringify\(\{ form, groupRows, keyRows \}\);/,
  "provider dirty-check snapshots should not compare transient row client ids",
);
assert.match(
  addProviderSource,
  /function normalizeProviderKeyRowsForDirtyCheck/,
  "provider dirty-check snapshots should normalize key rows before comparison",
);
assert.match(
  addProviderSource,
  /function normalizeProviderGroupRowsForDirtyCheck/,
  "provider dirty-check snapshots should normalize group rows before comparison",
);

console.log("page unsaved exit confirmation contract ok");
