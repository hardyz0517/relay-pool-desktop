import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
    dashboardSource.includes("requestKeyById") &&
    dashboardSource.includes("request.stationKeyId") &&
    dashboardSource.includes("requestStationName") &&
    dashboardSource.includes("stationNamesById"),
  "dashboard recent usage rows should resolve both station and key names",
);

assert(
  /\{request\.model \?\? request\.path\}[\s\S]*?\{formatDateTime\(request\.startedAt\)\}[\s\S]*?\{requestStationName\} · \{requestKeyName\}/.test(
    dashboardSource,
  ),
  "dashboard recent usage rows should show station and key names below the usage time",
);

assert(
  /min-w-\[88px\] text-right text-xs[\s\S]*?formatRecentRequestCost[\s\S]*?formatTokenCount\(request\.totalTokens\)/.test(dashboardSource),
  "dashboard recent usage rows should place cost above tokens on the right",
);
