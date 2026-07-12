import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.match(
  dashboardSource,
  /const proxyRequestCount = Math\.max\(\s*requestLogs\.length,\s*proxyStatus\?\.requestCount \?\? 0,\s*\);/,
  "dashboard cumulative request count should not drop below persisted request logs when the proxy runtime counter resets",
);

assert.doesNotMatch(
  dashboardSource,
  /const proxyRequestCount = proxyStatus\?\.requestCount \?\? requestLogs\.length;/,
  "dashboard should not prefer the ephemeral proxy runtime counter over persisted request logs",
);
