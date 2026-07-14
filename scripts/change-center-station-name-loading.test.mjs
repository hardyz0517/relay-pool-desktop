import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const esbuild = require("../node_modules/.pnpm/node_modules/esbuild");
const outFile = resolve(tmpdir(), "relay-pool-change-center-station-name-loading.test.mjs");

await mkdir(dirname(outFile), { recursive: true });
await esbuild.build({
  entryPoints: ["src/features/changes/changeEventViewModels.ts"],
  outfile: outFile,
  bundle: true,
  platform: "node",
  format: "esm",
});

const { buildChangeEventListItem } = await import(`${pathToFileURL(outFile).href}?t=${Date.now()}`);
const event = {
  id: "change-collector-failed",
  severity: "warning",
  eventType: "collector_failed",
  status: "unread",
  title: "站点采集失败",
  message: "upstream timeout",
  objectType: "station",
  objectId: "station-1783311325734-4639",
  stationId: "station-1783311325734-4639",
  stationKeyId: null,
  pricingRuleId: null,
  requestLogId: null,
  oldValueJson: null,
  newValueJson: JSON.stringify({ taskType: "groups" }),
  impactJson: null,
  dedupeKey: "collector_failed:station-1783311325734-4639:groups",
  source: "collector",
  detectedAt: "2026-07-14T00:00:00.000Z",
  resolvedAt: null,
  createdAt: "2026-07-14T00:00:00.000Z",
  updatedAt: "2026-07-14T00:00:00.000Z",
};

const pendingItem = buildChangeEventListItem(event, {
  stationNamesById: new Map(),
  deferStationIdentifierFallback: true,
});
assert.equal(
  pendingItem.title,
  "中转站 分组采集失败",
  "a pending station-name query should not expose an internal station ID",
);

const resolvedItem = buildChangeEventListItem(event, {
  stationNamesById: new Map([[event.stationId, "生产中转站"]]),
});
assert.equal(resolvedItem.title, "中转站 生产中转站 分组采集失败");

const missingItem = buildChangeEventListItem(event, { stationNamesById: new Map() });
assert.match(
  missingItem.title,
  /station-1783311\.\.\./,
  "a settled query should retain the existing identifier fallback for deleted stations",
);

const pageSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");
assert.match(
  pageSource,
  /deferStationIdentifierFallback=\{stationsQuery\.isPending\s*&&\s*stationsQuery\.data\s*===\s*undefined\}/,
  "the page should defer only the transient identifier fallback without blocking event rendering",
);

console.log("change center station-name loading contract passed");
