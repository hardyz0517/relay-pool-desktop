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
  /<section className="grid min-w-0 gap-3">\s*<header className="flex flex-wrap items-center justify-between gap-3">[\s\S]*?<StatusBadge[\s\S]*?<div className="grid min-w-0 grid-cols-4 gap-3">/,
  "current risk should keep four shrinkable columns without widening the page",
);

assert.match(
  dashboardSource,
  /activeRiskEvents\.length === 0[\s\S]*?<div className="grid min-w-0 gap-2">/,
  "current risk detail list should shrink inside the visible page width",
);

assert.match(
  dashboardSource,
  /<ObjectRow\s+key=\{event\.id\}\s+className="min-w-0"/,
  "each current risk detail row should shrink and truncate",
);

assert.ok(
  dashboardSource.includes(
    'className="flex min-h-[96px] min-w-0 items-center gap-3 rounded-[12px] border border-slate-200 bg-white px-4 py-3 shadow-[0_2px_8px_rgba(15,23,42,0.08)]"',
  ),
  "dashboard metric tiles should allow grid tracks to shrink instead of forcing horizontal overflow",
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
