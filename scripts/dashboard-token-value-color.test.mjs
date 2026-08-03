import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const metricPanelSource = await readFile("src/components/ui/MetricPanel.tsx", "utf8");

function metricBlock(label) {
  const start = dashboardSource.indexOf(`label: "${label}"`);
  assert.notEqual(start, -1, `dashboard should define the ${label} metric card`);
  const end = dashboardSource.indexOf("\n            },", start);
  assert.notEqual(end, -1, `dashboard should close the ${label} metric card object`);
  return dashboardSource.slice(start, end);
}

assert.ok(
  metricPanelSource.includes("valueClassName?: string;"),
  "MetricPanel metric items should allow a per-card value color override",
);

assert.ok(
  metricPanelSource.includes("valueClassName ??"),
  "MetricPanel should prefer valueClassName for the primary metric value when provided",
);

assert.match(
  dashboardSource,
  /label:\s*"总余额"[\s\S]*?tone:\s*lowBalanceStations > 0 \? "warning" : "good"[\s\S]*?valueClassName:\s*"text-success-foreground"[\s\S]*?accent:\s*"emerald"/,
  "dashboard total balance card should keep the primary balance value green while the icon tone carries the signal",
);

assert.match(
  metricBlock("今日请求"),
  /detail:\s*`总计：\$\{cumulativeRequestMetrics \? formatCompactNumber\(proxyRequestCount\) : cumulativeMetricsState\.value\}`[\s\S]*?valueClassName:\s*"text-foreground"[\s\S]*?accent:\s*"green"/,
  "dashboard today request card should render the primary request count in neutral text while keeping the green icon accent",
);

assert.match(
  dashboardSource,
  /label:\s*"今日 Token"[\s\S]*?valueClassName:\s*"text-foreground"[\s\S]*?accent:\s*"amber"/,
  "dashboard today Token card should render the primary value in neutral text while keeping the amber icon accent",
);

assert.match(
  dashboardSource,
  /label:\s*"累计 Token"[\s\S]*?valueClassName:\s*"text-foreground"[\s\S]*?accent:\s*"indigo"/,
  "dashboard total Token card should render the primary value in neutral text while keeping the indigo icon accent",
);

assert.match(
  dashboardSource,
  /label:\s*"平均总耗时"[\s\S]*?valueClassName:\s*"text-foreground"[\s\S]*?accent:\s*"rose"/,
  "dashboard average total duration card should render the primary value and unit in neutral text while keeping the rose icon accent",
);

assert.match(
  dashboardSource,
  /label:\s*"性能概览"[\s\S]*?<span className="text-foreground">\{formatCompactNumber\(recentPerformance\.rpm\)\}<\/span>[\s\S]*?<span className="ml-1 text-sm font-medium text-muted-foreground">RPM<\/span>[\s\S]*?valueClassName:\s*"inline-flex items-baseline text-foreground"[\s\S]*?accent:\s*"violet"/,
  "dashboard performance overview should render the RPM number in neutral text with a smaller unit label while keeping the violet icon accent",
);

assert.match(
  dashboardSource,
  /<span className="font-semibold text-foreground">\{formatCompactNumber\(recentPerformance\.tpm\)\}<\/span>[\s\S]*?<span className="ml-1 text-muted-foreground">TPM<\/span>[\s\S]*?<span className="text-muted-foreground">· \{activeRequests\} 活跃<\/span>/,
  "dashboard performance overview detail should render TPM with a separate unit label and keep the active request text muted",
);
