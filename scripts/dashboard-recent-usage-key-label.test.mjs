import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const recentUsageSource = dashboardSource.slice(
  dashboardSource.indexOf("recentUsageLogs.map"),
  dashboardSource.indexOf("</section>", dashboardSource.indexOf("recentUsageLogs.map")),
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  /<ModelMappingDisplay[\s\S]*?requestedModel=\{request\.model\}[\s\S]*?resolvedModel=\{request\.resolvedUpstreamModel\}[\s\S]*?fallback=\{request\.path\}[\s\S]*?layout="inline"[\s\S]*?formatRecentRequestCost[\s\S]*?\{formatDateTime\(request\.startedAt\)\}/.test(
    recentUsageSource,
  ),
  "dashboard recent usage rows should show inline model mapping with cost on the same line, above time and tokens",
);

assert(
  !recentUsageSource.includes("requestKeyById") &&
    !recentUsageSource.includes("requestKeyName") &&
    !recentUsageSource.includes("requestStationName") &&
    !recentUsageSource.includes("stationNamesById") &&
    !recentUsageSource.includes("stationName") &&
    !dashboardSource.includes("requestKeyById"),
  "dashboard recent usage rows should not show station provider or key identity",
);

assert(
  dashboardSource.includes('layout="inline"') &&
    dashboardSource.includes("items-baseline justify-between") &&
    dashboardSource.includes("formatRecentRequestCost") &&
    dashboardSource.includes("formatTokenCount(request.totalTokens)") &&
    !dashboardSource.includes("min-w-[88px] text-right text-xs"),
  "dashboard recent usage rows should place cost beside the model mapping and tokens beside the usage time",
);
