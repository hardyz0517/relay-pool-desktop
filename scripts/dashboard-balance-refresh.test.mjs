import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.ok(
  dashboardSource.includes("balanceSnapshotsQueryOptions"),
  "dashboard should read balances through the shared balance snapshot query option",
);

assert.ok(
  dashboardSource.includes("useActivityQuery(refreshEnabled, balanceSnapshotsQueryOptions())"),
  "dashboard balance refresh should be owned by the active query subscription",
);

assert.ok(
  !dashboardSource.includes("window.setInterval"),
  "dashboard should not own a page-local balance polling interval",
);
