import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.ok(
  dashboardSource.includes("dashboardLiveRequestMetricsQueryOptions(") &&
    dashboardSource.includes("dashboardCumulativeRequestMetricsQueryOptions(") &&
    dashboardSource.includes("const recentPerformance = liveRequestMetrics?.recent ?? null") &&
    dashboardSource.includes("const averageTotalDurationMs = todayMetrics?.avgTotalDurationMs ?? null"),
  "dashboard performance cards should be driven by the new live and cumulative dashboard snapshots",
);

assert.ok(
  dashboardSource.includes('label: "平均耗时"') &&
    dashboardSource.includes("formatAverageDurationDetail(todayMetrics)") &&
    dashboardSource.includes("TTFT"),
  "dashboard should distinguish average total duration from first-token latency",
);

assert.match(
  dashboardSource,
  /label:\s*"实时流量"[\s\S]*?<span className="text-foreground">\{formatCompactNumber\(recentPerformance\.rpm\)\}<\/span>[\s\S]*?<span className="ml-1 text-sm font-medium text-muted-foreground">RPM<\/span>[\s\S]*?valueClassName:\s*"inline-flex items-baseline text-foreground"[\s\S]*?accent:\s*"violet"/,
  "dashboard realtime traffic should render RPM as the primary value with a muted unit label",
);

assert.match(
  dashboardSource,
  /<span className="font-semibold text-foreground">\{formatCompactNumber\(recentPerformance\.tpm\)\}<\/span>[\s\S]*?<span className="ml-1 text-muted-foreground">TPM<\/span>[\s\S]*?<span className="text-muted-foreground">· \{activeRequests\} 活跃请求<\/span>/,
  "dashboard realtime traffic detail should render TPM and active requests inline without any old success-rate percent",
);

assert.ok(
  !dashboardSource.includes("getRecentPerformanceMetrics(") &&
    !dashboardSource.includes("formatPercent(todaySuccessRate)") &&
    !dashboardSource.includes("requestLogs.length / RECENT_PERFORMANCE_WINDOW_MINUTES"),
  "dashboard performance metrics should no longer depend on front-end log scanning",
);
