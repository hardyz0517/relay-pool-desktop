import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const resources = await readFile("src/lib/query/resourceQueries.ts", "utf8");

assert.match(
  resources,
  /export const stationAssetQueryOptions = \(stationId: string\) =>/,
  "station asset snapshots should be shared resource queries instead of page-local enrichment state",
);

assert.match(
  resources,
  /withQueryTimeout\(\s*getLatestCollectorSnapshot\(stationId\),\s*`station asset snapshot \$\{stationId\}`,\s*6_000,\s*\)/,
  "station asset snapshot queries should keep a bounded timeout",
);

assert.match(
  source,
  /useActivityQuery\(refreshEnabled,\s*stationsQueryOptions\(\)\)/,
  "station list reads should be gated by page refresh activity",
);

assert.match(
  source,
  /useActivityQuery\(\s*refreshEnabled,\s*currentStationBalanceSnapshotsQueryOptions\(\),\s*\)/,
  "balance snapshot reads should be gated by page refresh activity",
);

assert.match(
  source,
  /useActivityQuery\(refreshEnabled,\s*changeEventsQueryOptions\(false\)\)/,
  "change-event reads should be gated by page refresh activity",
);

assert.ok(
  source.includes("useQueries") &&
    source.includes("stationAssetQueryOptions(station.id)") &&
    source.includes("subscribed: refreshEnabled"),
  "station asset snapshot reads should unsubscribe when the page is hidden",
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
