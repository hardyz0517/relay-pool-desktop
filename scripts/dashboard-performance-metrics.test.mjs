import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.ok(
  dashboardSource.includes("const recentPerformance = getRecentPerformanceMetrics(requestLogs);"),
  "dashboard should derive the performance card from recent request-log throughput",
);

assert.ok(
  dashboardSource.includes('value: `${formatCompactNumber(recentPerformance.rpm)} RPM`'),
  "dashboard performance overview should show RPM as the primary value",
);

assert.ok(
  dashboardSource.includes("`${formatCompactNumber(recentPerformance.tpm)} TPM · ${activeRequests} 活跃`"),
  "dashboard performance overview should show TPM in the detail line",
);

assert.doesNotMatch(
  dashboardSource,
  /label:\s*"性能概览"[\s\S]*?value:\s*formatPercent\(todaySuccessRate\)/,
  "dashboard performance overview should not display success rate as a percentage",
);

assert.ok(
  dashboardSource.includes("const RECENT_PERFORMANCE_WINDOW_MINUTES = 5;") &&
    dashboardSource.includes("function getRecentPerformanceMetrics(logs: RequestLog[])") &&
    dashboardSource.includes("rpm: recentLogs.length / RECENT_PERFORMANCE_WINDOW_MINUTES") &&
    dashboardSource.includes("tpm: recentTokens / RECENT_PERFORMANCE_WINDOW_MINUTES"),
  "dashboard performance metrics should use the recent 5-minute RPM/TPM window",
);

assert.ok(
  dashboardSource.includes("const DASHBOARD_RUNTIME_REFRESH_INTERVAL_MS = 2_000;") &&
    dashboardSource.includes('import { getProxyStatus, listRequestLogs, startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";'),
  "dashboard should import and define a short runtime refresh path for live request throughput",
);

assert.ok(
  dashboardSource.includes("async function refreshDashboardRuntimeFacts()") &&
    dashboardSource.includes("const [nextProxyStatus, nextRequestLogs] = await Promise.all([") &&
    dashboardSource.includes("getProxyStatus(),") &&
    dashboardSource.includes("listRequestLogs(),") &&
    dashboardSource.includes("setProxyStatus(nextProxyStatus)") &&
    dashboardSource.includes("setRequestLogs(nextRequestLogs)"),
  "dashboard should refresh proxy status and request logs together for live performance metrics",
);

assert.ok(
  dashboardSource.includes("const runtimeRefreshIntervalId = window.setInterval(") &&
    dashboardSource.includes("refreshDashboardRuntimeFacts") &&
    dashboardSource.includes("DASHBOARD_RUNTIME_REFRESH_INTERVAL_MS") &&
    dashboardSource.includes("window.clearInterval(runtimeRefreshIntervalId)"),
  "dashboard should keep performance metrics fresh with a cleared interval",
);
