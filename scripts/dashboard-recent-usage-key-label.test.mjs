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
  "dashboard recent usage rows should resolve station names from the request and key lookup",
);

assert(
  /<ModelMappingDisplay[\s\S]*?requestedModel=\{request\.model\}[\s\S]*?resolvedModel=\{request\.resolvedUpstreamModel\}[\s\S]*?fallback=\{request\.path\}[\s\S]*?layout="inline"[\s\S]*?formatRecentRequestCost[\s\S]*?\{formatDateTime\(request\.startedAt\)\}[\s\S]*?\{requestStationName\}/.test(
    dashboardSource,
  ) &&
    !dashboardSource.includes("requestKeyName") &&
    !dashboardSource.includes("{requestStationName} · {requestKeyName}"),
  "dashboard recent usage rows should show inline model mapping with cost, then time and station name without the specific key",
);

assert(
  dashboardSource.includes('layout="inline"') &&
    dashboardSource.includes("items-baseline justify-between") &&
    dashboardSource.includes("formatRecentRequestCost") &&
    dashboardSource.includes("formatTokenCount(request.totalTokens)") &&
    !dashboardSource.includes("min-w-[88px] text-right text-xs"),
  "dashboard recent usage rows should place cost beside the model mapping and tokens beside the usage time",
);
