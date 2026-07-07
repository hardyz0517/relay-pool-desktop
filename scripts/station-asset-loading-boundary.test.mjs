import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationsPage.tsx", "utf8");

assert.ok(
  source.includes("STATION_ASSET_PRIMARY_TIMEOUT_MS"),
  "station asset primary list read should have a bounded timeout instead of an endless loading state",
);

assert.match(
  source,
  /const\s+nextStations\s*=\s*await\s+withStationAssetTimeout\(\s*listStations\(\)/,
  "station asset initial load should make listStations the only blocking read for the first screen",
);

assert.ok(
  !source.includes("const [nextStations, nextBalances, nextChanges] = await Promise.all"),
  "station asset initial load should not wait for balances and change events before leaving loading",
);

assert.ok(
  source.includes("void refreshStationAssetEnrichment(nextStations") &&
    source.indexOf("setStations(nextStations)") < source.indexOf("void refreshStationAssetEnrichment(nextStations"),
  "station asset enrichment should start only after the station list has been committed",
);

assert.ok(
  source.includes("Promise.allSettled") &&
    source.includes("listBalanceSnapshots()") &&
    source.includes("listChangeEvents()") &&
    source.includes("getLatestCollectorSnapshot(station.id)"),
  "station asset enrichment should tolerate partial failures across balances, changes, and collector snapshots",
);
