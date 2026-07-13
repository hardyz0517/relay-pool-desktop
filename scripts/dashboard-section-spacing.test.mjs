import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const metricPanelSource = await readFile("src/components/ui/MetricPanel.tsx", "utf8");

assert.ok(
  metricPanelSource.includes('<section className={cn("grid gap-3", className)}>') &&
    metricPanelSource.includes('<div className="grid gap-3 sm:grid-cols-2 md:grid-cols-4">'),
  "metric panels should keep a 12px title-to-card and card-grid rhythm",
);

assert.match(
  dashboardSource,
  /<section className="grid gap-3">\s*<header className="flex flex-wrap items-center justify-between gap-3">[\s\S]*?<StatusBadge[\s\S]*?<div className="grid gap-3 sm:grid-cols-2 md:grid-cols-4">/,
  "current risk section should use the same 12px spacing and responsive grid as dashboard metric panels",
);

assert.ok(
  dashboardSource.includes(
    'className="flex min-h-[96px] items-center gap-3 rounded-[12px] border border-slate-200 bg-white px-4 py-3 shadow-[0_2px_8px_rgba(15,23,42,0.08)]"',
  ),
  "dashboard metric tiles should match the white elevated card style used by MetricPanel cards",
);

assert.ok(
  dashboardSource.includes(
    "flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]",
  ),
  "dashboard metric tile icons should match the metric panel icon container shape",
);

assert.match(
  dashboardSource,
  /<h2 className="truncate text-\[13px\] font-semibold text-slate-800">\s*秘钥健康\s*<\/h2>\s*<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">[\s\S]*?stationKeyStatusLabels/,
  "key health cards should use the requested title and desktop five-column rhythm",
);

assert.doesNotMatch(
  dashboardSource,
  /recentError|已知余额总计/,
  "key health should not render the bottom balance and recent-error summary line",
);

assert.doesNotMatch(
  dashboardSource,
  /<SectionCard\s+title=(?:(?!<\/SectionCard>)[\s\S])*?<DashboardMetricTile/,
  "dashboard metric tiles should not fall back to padded SectionCard wrappers",
);
