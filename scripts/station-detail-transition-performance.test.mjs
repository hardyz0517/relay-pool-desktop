import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationDetailPage.tsx", "utf8");
const appSource = await readFile("src/app/App.tsx", "utf8");

assert.ok(
  !source.includes("useLayoutEffect"),
  "seeded station detail reads should not run in a layout effect that delays the first paint",
);

assert.match(
  source,
  /useEffect\(\(\) => \{[\s\S]*activeStationIdRef\.current = stationId;[\s\S]*void loadDetail\(stationId, "silent"\)/,
  "station detail should paint its seed first, then refresh persisted detail data in a passive effect",
);

assert.ok(
  appSource.includes('key={detailStationId ?? "station-detail-empty"}'),
  "rapidly opening another station during exit should remount detail state for the new station identity",
);

console.log("station detail transition performance contract ok");
