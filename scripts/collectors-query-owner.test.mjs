import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const collectorsPage = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");
const queryKeys = await readFile("src/lib/query/queryKeys.ts", "utf8");
const resourceQueries = await readFile("src/lib/query/resourceQueries.ts", "utf8");

for (const expected of [
  "stationsQueryOptions()",
  "stationAssetQueryOptions(selectedCollectorStationId)",
  "collectorSnapshotsQueryOptions(selectedCollectorStationId)",
  "captureSessionStatusQueryOptions(selectedCollectorStationId)",
  "collectorRunsQueryOptions(selectedCollectorStationId)",
]) {
  assert.ok(
    collectorsPage.includes(expected),
    `CollectorsPage should read ${expected} through canonical query options`,
  );
}

for (const forbidden of [
  "usePageActivation",
  "setStations",
  "setLatestSnapshot",
  "setHistory",
  "setLoading",
  "setCaptureStatus",
  "setRuns",
  "Promise.all([",
  "listStations(",
  "getLatestCollectorSnapshot(",
  "listCollectorSnapshots(",
  "getCaptureSessionStatus(",
  "listCollectorRuns(",
]) {
  assert.ok(
    !collectorsPage.includes(forbidden),
    `CollectorsPage should not keep legacy local server-state/read path: ${forbidden}`,
  );
}

for (const expected of [
  "collectorSnapshots: (stationId: string)",
  "collectorRuns: (stationId: string)",
  "captureSessionStatus: (stationId: string)",
]) {
  assert.ok(queryKeys.includes(expected), `queryKeys should expose ${expected}`);
}

for (const expected of [
  "export const collectorSnapshotsQueryOptions",
  "export const collectorRunsQueryOptions",
  "export const captureSessionStatusQueryOptions",
]) {
  assert.ok(resourceQueries.includes(expected), `resourceQueries should expose ${expected}`);
}

console.log("collectors query owner contract ok");
