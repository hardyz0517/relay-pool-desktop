import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");

assert.ok(
  !source.includes("const scanRemoteDisabled =\n    !editing ||"),
  "new provider page should not disable fetching remote keys just because the supplier is unsaved",
);

assert.ok(
  source.includes("ensureStationForRemoteKeyActions"),
  "remote key actions should ensure an unsaved supplier is persisted before scanning",
);

assert.match(
  source,
  /const targetStationId = await ensureStationForRemoteKeyActions\(\);[\s\S]*scanRemoteStationKeys\(targetStationId\)/,
  "fetch all keys should scan the ensured station id",
);

assert.ok(
  source.includes("{activeStationId && ("),
  "remote discovery list should become visible after the new supplier is auto-saved",
);
