import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.ok(
  dashboardSource.includes("const recentPerformance = getRecentPerformanceMetrics(requestLogs);"),
  "dashboard should derive the performance card from recent request-log throughput",
);

assert.ok(
  dashboardSource.includes('<span className="text-slate-900">{formatCompactNumber(recentPerformance.rpm)}</span>') &&
    dashboardSource.includes('<span className="ml-1 text-sm font-medium text-muted-foreground">RPM</span>'),
  "dashboard performance overview should show RPM as the primary value with a separated unit label",
);

assert.ok(
  dashboardSource.includes('<span className="font-semibold text-slate-900">{formatCompactNumber(recentPerformance.tpm)}</span>') &&
    dashboardSource.includes('<span className="ml-1 text-muted-foreground">TPM</span>') &&
    dashboardSource.includes('<span className="text-muted-foreground">· {activeRequests} 活跃</span>'),
  "dashboard performance overview should show TPM in the detail line with a separated unit label",
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
  dashboardSource.includes("requestLogsQueryOptions(proxyStatusQuery.data?.running ? 2_000 : false)") &&
    dashboardSource.includes("proxyStatusQueryOptions(false)"),
  "dashboard should keep live request throughput fresh through shared query options",
);

assert.ok(
  dashboardSource.includes("const requestLogs = requestLogsQuery.data ?? []") &&
    dashboardSource.includes("const proxyStatus = proxyStatusQuery.data ?? null"),
  "dashboard should derive live performance metrics from shared query data",
);

assert.ok(
  !dashboardSource.includes("refreshDashboardRuntimeFacts") &&
    !dashboardSource.includes("window.setInterval"),
  "dashboard should not own a runtime refresh interval",
);
