import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const keyPoolSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const editKeySource = await readFile("src/features/key-pool/EditKeyPage.tsx", "utf8");
const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const stationKeyRowsEditorSource = await readFile(
  "src/features/stations/components/StationKeyRowsEditor.tsx",
  "utf8",
);
const groupOptionSource = await readFile(
  "src/features/stations/groupOptionViewModels.ts",
  "utf8",
);

assert.ok(
  groupOptionSource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"),
  "group option helpers should expose a select-ready current group facts adapter",
);

assert.ok(
  keyPoolSource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"),
  "key pool group binding selector should consume current group fact options",
);

assert.ok(
  editKeySource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"),
  "edit-key group binding selector should consume current group fact options",
);

assert.ok(
  addProviderSource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"),
  "add-provider key group selectors should consume current group fact options",
);

assert.ok(
  keyPoolSource.includes("KEEP_GROUP_BINDING_VALUE") &&
    keyPoolSource.includes("CLEAR_GROUP_BINDING_VALUE"),
  "key pool edits should preserve explicit keep and clear binding actions",
);

assert.ok(
  editKeySource.includes("KEEP_GROUP_BINDING_VALUE") &&
    editKeySource.includes("CLEAR_GROUP_BINDING_VALUE"),
  "edit-key edits should preserve explicit keep and clear binding actions",
);

assert.ok(
  !stationKeyRowsEditorSource.includes("rateSource: null"),
  "station key row draft fallback should not erase current group rate source metadata",
);
