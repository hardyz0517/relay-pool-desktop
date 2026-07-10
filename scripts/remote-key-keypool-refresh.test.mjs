import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stationKeysApiSource = await readFile("src/lib/api/stationKeys.ts", "utf8");
const keyPoolPageSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");

assert.ok(
  stationKeysApiSource.includes("KEY_POOL_ITEMS_UPDATED_EVENT"),
  "station key API should expose a shared key-pool invalidation event",
);

assert.match(
  stationKeysApiSource,
  /createLocalStationKeyFromRemote[\s\S]*withKeyPoolItemsInvalidation\(/,
  "creating a local key from a remote discovery should invalidate the mounted key pool",
);

assert.match(
  stationKeysApiSource,
  /deleteStationKey[\s\S]*withKeyPoolItemsInvalidation\(/,
  "deleting a station key outside the key-pool page should invalidate the mounted key pool",
);

assert.match(
  keyPoolPageSource,
  /window\.addEventListener\(KEY_POOL_ITEMS_UPDATED_EVENT,\s*handleKeyPoolItemsUpdated\)/,
  "KeyPoolPage should refresh when another page changes station keys",
);

assert.match(
  keyPoolPageSource,
  /window\.removeEventListener\(KEY_POOL_ITEMS_UPDATED_EVENT,\s*handleKeyPoolItemsUpdated\)/,
  "KeyPoolPage should remove the cross-page refresh listener on unmount",
);

assert.ok(
  addProviderSource.includes("createLocalStationKeyFromRemote(remoteKey.id, targetStationId)"),
  "remote local-key toggle should still use the backend full-secret sync path",
);
