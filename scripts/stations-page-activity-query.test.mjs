import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const resources = await readFile("src/lib/query/resourceQueries.ts", "utf8");

assert.match(source, /stationsQueryOptions/);
assert.match(source, /balanceSnapshotsQueryOptions/);
assert.match(source, /changeEventsQueryOptions/);
assert.match(source, /useQueries/);
assert.match(resources, /stationAssetQueryOptions/);
assert.match(resources, /withQueryTimeout/);
assert.ok(!source.includes("STATION_ASSET_REFRESH_INTERVAL_MS"));
assert.ok(!source.includes("window.setInterval"));
assert.ok(!source.includes("refreshStationAssetEnrichment"));

const { withQueryTimeout } = await import("../src/lib/query/withQueryTimeout.ts");

{
  const startedAt = Date.now();
  await assert.rejects(
    withQueryTimeout(new Promise(() => {}), "never settles", 15),
    /never settles timed out after 15ms/,
  );
  assert.ok(Date.now() - startedAt >= 10, "timeout should wait for the configured duration");
}

{
  const result = await withQueryTimeout(Promise.resolve("ok"), "resolves", 100);
  assert.equal(result, "ok");
}

console.log("stations page activity query contract passed");
