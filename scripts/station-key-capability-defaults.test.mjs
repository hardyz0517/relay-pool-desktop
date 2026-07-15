import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const defaults = readFileSync("src/lib/stationKeyCapabilityDefaults.ts", "utf8");
const featureDefaults = readFileSync("src/features/key-pool/stationKeyCapabilityDefaults.ts", "utf8");
const editPage = readFileSync("src/features/key-pool/EditKeyPage.tsx", "utf8");
const poolPage = readFileSync("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const stationKeysApi = readFileSync("src/lib/api/stationKeys.ts", "utf8");

assert.match(defaults, /OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS/);
assert.match(featureDefaults, /@\/lib\/stationKeyCapabilityDefaults/);
assert.match(poolPage, /schedulable/);
assert.doesNotMatch(editPage, /supportsTools:\s*true/);
assert.doesNotMatch(editPage, /supportsReasoning:\s*true/);
assert.match(editPage, /getStationKeyCapabilities/);
assert.match(stationKeysApi, /OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS\.supportsTools/);
assert.match(stationKeysApi, /OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS\.supportsReasoning/);

console.log("station key capability defaults contract passed");
