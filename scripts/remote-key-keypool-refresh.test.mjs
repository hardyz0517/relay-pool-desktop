import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stationKeysApiSource = await readFile("src/lib/api/stationKeys.ts", "utf8");
const keyPoolPageSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");

assert.match(
  stationKeysApiSource,
  /getActiveBackendClient\(\)\.stationKeys/,
  "station key API should route station-key operations through the active backend facade",
);

assert.match(
  stationKeysApiSource,
  /createLocalStationKeyFromRemote\(remoteKeyId,\s*stationId\)/,
  "creating a local key from a remote discovery should keep the backend full-secret sync path",
);

assert.match(
  stationKeysApiSource,
  /deleteStationKey\(id\)/,
  "deleting a station key should keep the backend station-key delete path",
);

assert.ok(
  keyPoolPageSource.includes("useActivityQuery(keyPoolQueryOptions())") &&
    keyPoolPageSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.keyPool })") &&
    !keyPoolPageSource.includes("KEY_POOL_ITEMS_UPDATED_EVENT") &&
    !keyPoolPageSource.includes("handleKeyPoolItemsUpdated"),
  "KeyPoolPage should own key-pool reads through Query Cache, without DOM invalidation events",
);

assert.ok(
  addProviderSource.includes("createLocalStationKeyFromRemote(remoteKey.id, targetStationId)") &&
    addProviderSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.keyPool })") &&
    addProviderSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.stations })") &&
    addProviderSource.includes("await invalidateProviderWorkspaceCaches()"),
  "remote local-key toggle should still sync via backend and invalidate canonical provider workspace queries",
);
