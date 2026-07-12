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
    source.indexOf("setStations((currentStations)") <
      source.indexOf("void refreshStationAssetEnrichment(nextStations"),
  "station asset enrichment should start only after the station list has been committed",
);

assert.ok(
  source.includes("Promise.allSettled") &&
    source.includes("listBalanceSnapshots()") &&
    source.includes("listChangeEvents()") &&
    source.includes("getLatestCollectorSnapshot(station.id)"),
  "station asset enrichment should tolerate partial failures across balances, changes, and collector snapshots",
);

assert.ok(
  source.includes("areStationAssetListsEqual(currentStations, nextStations)") &&
    source.includes("return currentStations;"),
  "silent station asset refresh should keep the existing station array when list data is unchanged",
);

assert.ok(
  source.includes("shouldAnimateStationAssetLayoutChanges") &&
    source.includes("animateLayoutChanges: shouldAnimateStationAssetLayoutChanges") &&
    source.includes("isSorting || wasDragging"),
  "station rows should not run sortable layout animations for background refreshes or return-navigation reactivation",
);

assert.match(
  source,
  /async function handleRunCollect\([\s\S]*?await collectSub2apiStation\(station\.id\);[\s\S]*?await refreshStations\(\{ silent: true \}\);/,
  "manual station collection should refresh the asset list silently so the page does not swap to the loading state and jump to the top",
);

assert.match(
  source,
  /async function handleRefreshBalance\([\s\S]*?collectStationTask\(station\.id,\s*"balance"\);[\s\S]*?await refreshStations\(\{ silent: true \}\);/,
  "manual balance refresh should refresh the asset list silently so the current scroll position is preserved",
);
