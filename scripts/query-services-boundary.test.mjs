import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const queriesDir = path.join(root, "src", "lib", "queries");
const queryFiles = (await readdir(queriesDir))
  .filter((fileName) => fileName.endsWith(".ts"))
  .sort();

assert.deepEqual(
  queryFiles,
  ["changeQueries.ts", "channelQueries.ts", "dashboardQueries.ts", "localRoutingQueries.ts", "logQueries.ts", "routingQueries.ts"],
  "Stage 2 query service inventory should be explicit until the next slice adds another reviewed query module",
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
    pattern: /\b(simulateRoute|upsertModelAlias|deleteModelAlias|updateSettings)\b/,
    reason: "query services must not perform routing decisions or write actions",
  },
  {
    pattern: /\b(filterLogsByWindow|buildChannels|orderChannelsBySavedOrder|runChannelMonitorNow|createChannelMonitor|updateChannelMonitor|deleteChannelMonitor)\b/,
    reason: "query services must not define channel view behavior or channel write actions",
  },
];

for (const fileName of queryFiles) {
  const relativePath = `src/lib/queries/${fileName}`;
  const source = await readFile(path.join(queriesDir, fileName), "utf8");

  assert.match(
    source,
    /(?:export\s+type\s+\w+Workspace\b|import\s+type\s+\{\s*\w+Workspace\s*\})/,
    `${relativePath} should declare or explicitly import its raw facts workspace type`,
  );
  assert.match(
    source,
    /export\s+(?:async\s+)?function\s+load\w+Workspace\(/,
    `${relativePath} should expose a load*Workspace query function`,
  );

  for (const { pattern, reason } of forbiddenPatterns) {
    assert.ok(!pattern.test(source), `${relativePath}: ${reason}`);
  }
}
