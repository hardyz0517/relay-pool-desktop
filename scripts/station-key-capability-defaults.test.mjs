import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const defaults = readFileSync("src/lib/stationKeyCapabilityDefaults.ts", "utf8");
const featureDefaults = readFileSync("src/features/key-pool/stationKeyCapabilityDefaults.ts", "utf8");
const editPage = readFileSync("src/features/key-pool/EditKeyPage.tsx", "utf8");
const formModel = readFileSync("src/features/key-pool/KeyPoolFormModel.tsx", "utf8");
const pageController = readFileSync("src/features/key-pool/useKeyPoolPageController.ts", "utf8");
const stationKeysApi = readFileSync("src/lib/api/stationKeys.ts", "utf8");

assert.match(defaults, /OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS/);
assert.match(defaults, /supportsTools:\s*true/);
assert.match(defaults, /supportsReasoning:\s*true/);
assert.match(featureDefaults, /@\/lib\/stationKeyCapabilityDefaults/);
assert.doesNotMatch(editPage, /supportsTools:\s*true/);
assert.doesNotMatch(editPage, /supportsReasoning:\s*true/);
assert.match(editPage, /getStationKeyCapabilities/);
assert.match(formModel, /schedulable:\s*true/);
assert.match(formModel, /supportsTools:\s*OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS\.supportsTools/);
assert.match(formModel, /supportsReasoning:\s*OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS\.supportsReasoning/);
assert.match(formModel, /capabilitiesFromEditForm/);
assert.match(pageController, /schedulable:\s*editForm\.schedulable/);
assert.match(pageController, /capabilities:\s*capabilitiesFromEditForm\(editForm\)/);
assert.doesNotMatch(stationKeysApi, /OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS/);

console.log("station key capability defaults contract passed");
