import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.ok(
  dashboardSource.includes("DASHBOARD_BALANCE_REFRESH_INTERVAL_MS"),
  "dashboard should define a balance refresh interval for CCSwitch parity",
);

assert.ok(
  dashboardSource.includes("window.setInterval") &&
    dashboardSource.includes("listBalanceSnapshots") &&
    dashboardSource.includes("setBalanceSnapshots"),
  "dashboard should periodically reload balance snapshots instead of only reading them on mount",
);

assert.ok(
  dashboardSource.includes("window.clearInterval"),
  "dashboard balance polling should clean up its interval",
);
