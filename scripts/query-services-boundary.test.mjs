import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const queriesDir = path.join(root, "src", "lib", "queries");
const queryFiles = (await readdir(queriesDir))
  .filter((fileName) => fileName.endsWith(".ts") && !fileName.endsWith(".test.ts"))
  .sort();

assert.deepEqual(
  queryFiles,
  [
    "alertingQueries.ts",
    "channelQueries.ts",
    "pricingQueries.ts",
    "routingQueries.ts",
  ],
  "query service inventory should be explicit until the next reviewed module is added",
);

const forbiddenPatterns = [
  {
    pattern: /from\s+"@\/features\//,
    reason: "query services must not import feature page/view-model modules",
  },
  {
    pattern: /from\s+"@\/lib\/projections\//,
    reason: "query services must not consume projections while Stage 2 is only raw fact loading",
  },
  {
    pattern: /\b(summarizeDashboardBalances|filterChangeEvents|paginateChangeEvents|unreadRiskCount|buildChangeEventListItem)\b/,
    reason: "query services must not define dashboard/change center view-model behavior",
  },
  {
    pattern: /\b(buildPricingComparisonViewModel|buildStationAssetRows|buildStationDetailViewModel)\b/,
    reason: "query services must not call feature projections or page view-model builders",
  },
  {
    pattern: /\b(getLocalAccessKey|markChangeEventRead|markUnreadChangeEventsRead|clearChangeEvents)\b/,
    reason: "query services must not eagerly read secrets or perform write actions",
  },
  {
    pattern: /\b(clearRequestLogs)\b/,
    reason: "query services must not perform request-log write actions",
  },
  {
    pattern: /\b(upsertModelAlias|deleteModelAlias|updateSettings)\b/,
    reason: "query services must not perform routing write actions",
  },
  {
    pattern: /\b(filterLogsByWindow|buildChannels|orderChannelsBySavedOrder|runChannelMonitorNow|createChannelMonitor|updateChannelMonitor|deleteChannelMonitor)\b/,
    reason: "query services must not define channel view behavior or channel write actions",
  },
];

for (const fileName of queryFiles) {
  const relativePath = `src/lib/queries/${fileName}`;
  const source = await readFile(path.join(queriesDir, fileName), "utf8");

  for (const { pattern, reason } of forbiddenPatterns) {
    assert.ok(!pattern.test(source), `${relativePath}: ${reason}`);
  }
}
