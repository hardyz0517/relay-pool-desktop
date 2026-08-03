import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

await assert.rejects(
  access("src/lib/queries/dashboardQueries.ts"),
  "legacy dashboard composite query service should be deleted once Dashboard owns explicit query options",
);

assert.ok(
  !dashboardSource.includes("loadDashboardWorkspace"),
  "dashboard should no longer import or call the legacy composite workspace loader",
);

for (const option of [
  "dashboardLiveRequestMetricsQueryOptions",
  "dashboardCumulativeRequestMetricsQueryOptions",
  "proxyStatusQueryOptions",
  "requestLogsQueryOptions",
  "keyPoolQueryOptions",
  "stationsQueryOptions",
  "currentStationBalanceSnapshotsQueryOptions",
  "settingsQueryOptions",
  "changeEventsQueryOptions",
]) {
  assert.ok(dashboardSource.includes(option), `dashboard should consume ${option}`);
}

assert.ok(
  !/void\s+Promise\.all\(\[\s*getProxyStatus\(\),\s*listRequestLogs\(\),\s*listKeyPoolItems\(\),\s*listBalanceSnapshots\(\),\s*getSettings\(\),\s*listChangeEvents\(\),?\s*\]\)/s.test(
    dashboardSource,
  ),
  "dashboard page should no longer own the initial raw fact Promise.all orchestration",
);
