import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationsPage.tsx", "utf8");

assert.ok(
  !source.includes("?? stations[0] ?? null"),
  "station asset list should not synthesize a selected row from the first station",
);

assert.ok(
  !source.includes("return nextStations[0]?.id ?? null;"),
  "station refresh should clear a missing selection instead of selecting the first station",
);

assert.match(
  source,
  /active=\{row\.station\.id === selectedStationId\}/,
  "station row highlight should be tied to an explicit selected station id",
);
