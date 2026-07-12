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
  /<section className="grid gap-3">\s*<header className="flex flex-wrap items-center justify-between gap-3">\s*<h2 className="truncate text-\[13px\] font-semibold text-slate-800">\s*当前风险\s*<\/h2>[\s\S]*?<div className="grid gap-3 md:grid-cols-4">/,
  "current risk section should use the same 12px title-to-card spacing as dashboard metric panels",
);

for (const sectionTitle of ["路由队列", "最近使用", "Key 健康"]) {
  assert.match(
    dashboardSource,
    new RegExp(
      `<section className="grid gap-3">\\s*<h2 className="truncate text-\\[13px\\] font-semibold text-slate-800">\\s*${sectionTitle}\\s*<\\/h2>\\s*<div className="grid gap-3`,
    ),
    `${sectionTitle} should use the same 12px title-to-card spacing as dashboard metric panels`,
  );
}

assert.doesNotMatch(
  dashboardSource,
  /<SectionCard\s+title="当前风险"[\s\S]*?<div className="mb-3 grid gap-2 md:grid-cols-4">/,
  "current risk summary should not use the padded SectionCard spacing that pushes cards farther from the title",
);

assert.doesNotMatch(
  dashboardSource,
  /<SectionCard\s+title="(?:路由队列|最近使用|Key 健康)"/,
  "lower dashboard sections should not use the padded SectionCard header spacing",
);
