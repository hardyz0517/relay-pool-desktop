import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.match(
  dashboardSource,
  /const proxyRequestCount = Math\.max\(lifetimeMetrics\?\.requestCount \?\? 0, proxyStatus\?\.requestCount \?\? 0\);/,
  "dashboard cumulative request count should not drop below the persisted lifetime snapshot when the proxy runtime counter resets",
);

assert.doesNotMatch(
  dashboardSource,
  /const proxyRequestCount = proxyStatus\?\.requestCount \?\? lifetimeMetrics\.requestCount;/,
  "dashboard should not prefer the ephemeral proxy runtime counter over the cumulative read model",
);
