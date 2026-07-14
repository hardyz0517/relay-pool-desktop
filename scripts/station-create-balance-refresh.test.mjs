import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stationsPageSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");

assert.ok(
  /const\s+nextStation\s*=\s*await\s+createStation\(input\);[\s\S]*?collectStationTask\(nextStation\.id,\s*"balance"\)/.test(
    stationsPageSource,
  ),
  "creating a station should immediately run a balance collection for the new station",
);

assert.ok(
  /collectStationTask\(nextStation\.id,\s*"balance"\)[\s\S]*?await invalidateStationSharedQueries\(\)/.test(
    stationsPageSource,
  ),
  "station list and balance snapshots should invalidate after the creation-time balance collection",
);
