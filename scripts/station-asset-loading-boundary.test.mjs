import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const resources = await readFile("src/lib/query/resourceQueries.ts", "utf8");

assert.match(
  resources,
  /export const stationAssetQueryOptions = \(stationId: string\) =>/,
  "station detail snapshot reads should keep a shared single-station resource query",
);

assert.match(
  resources,
  /withQueryTimeout\(\s*getLatestCollectorSnapshot\(stationId\),\s*`station asset snapshot \$\{stationId\}`,\s*6_000,\s*\)/,
  "single-station detail snapshot queries should keep a bounded timeout",
);

assert.match(
  source,
  /useActivityQuery\(stationsQueryOptions\(\)\)/,
  "station list reads should be gated by canonical page query activity",
);

assert.match(
  source,
  /useActivityQuery\(\s*currentStationBalanceSnapshotsQueryOptions\(\),\s*\)/,
  "balance snapshot reads should be gated by canonical page query activity",
);

assert.match(
  source,
  /useActivityQuery\(changeEventsQueryOptions\(false\)\)/,
  "change-event reads should be gated by canonical page query activity",
);

assert.ok(
  source.includes("stationAssetsQueryOptions(stationIds)") &&
    source.includes("new Map(") &&
    !source.includes("stationAssetQueryOptions(station.id)") &&
    !source.includes("useQueries"),
  "station list asset snapshots should read one bounded aggregate query instead of per-row queries",
);

assert.ok(
  resources.includes("export const stationAssetsQueryOptions = (stationIds: readonly string[]) =>") &&
    resources.includes("listLatestCollectorSnapshots([...stationIds])") &&
    resources.includes("enabled: stationIds.length > 0") &&
    resources.includes('queryKey: queryKeys.stationAssetsForStations(stationIds)') &&
    resources.includes('"station asset snapshots"'),
  "station list aggregate snapshots should use a canonical bounded query option",
);

assert.ok(
  !source.includes("window.setInterval") &&
    !source.includes("refreshStationAssetEnrichment") &&
    !source.includes("withStationAssetTimeout"),
  "station assets should not keep a page-local interval or hidden enrichment loop",
);

assert.ok(
  source.includes("shouldAnimateStationAssetLayoutChanges") &&
    source.includes("animateLayoutChanges: shouldAnimateStationAssetLayoutChanges") &&
    source.includes("isSorting || wasDragging"),
  "station rows should not run sortable layout animations for background refreshes or return-navigation reactivation",
);

assert.match(
  source,
  /async function handleRunCollect\([\s\S]*?await collectSub2apiStation\(station\.id\);[\s\S]*?await invalidateStationSharedQueries\(\);/,
  "manual station collection should refresh shared station facts without swapping the page to a loading state",
);

assert.match(
  source,
  /async function handleRefreshBalance\([\s\S]*?collectStationTask\(station\.id,\s*"balance"\);[\s\S]*?await invalidateStationSharedQueries\(\);/,
  "manual balance refresh should refresh shared station facts without swapping the page to a loading state",
);

console.log("station asset loading boundary contract passed");
