import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const metricPanelSource = await readFile("src/components/ui/MetricPanel.tsx", "utf8");
const stationDetailViewModelSource = await readFile(
  "src/features/stations/stationDetailViewModels.ts",
  "utf8",
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  dashboardSource.includes('title="中转站指标统计"') &&
    dashboardSource.includes('title="本地路由指标"') &&
    dashboardSource.includes("stationUsage.todayInputTokenCount") &&
    dashboardSource.includes("stationUsage.todayOutputTokenCount") &&
    dashboardSource.includes("stationUsage.totalInputTokenCount") &&
    dashboardSource.includes("stationUsage.totalOutputTokenCount") &&
    dashboardSource.includes("输入:") &&
    dashboardSource.includes("输出:"),
  "dashboard station usage token cards should show input/output token breakdowns",
);

assert(
  !dashboardSource.includes('description="来自中转站后台采集，不含本地代理日志"'),
  "dashboard station usage section should not render the old explanatory description",
);

assert(
  stationDetailViewModelSource.includes("function formatTokenBreakdown") &&
    stationDetailViewModelSource.includes("todayInputTokenCount") &&
    stationDetailViewModelSource.includes("todayOutputTokenCount") &&
    stationDetailViewModelSource.includes("totalInputTokenCount") &&
    stationDetailViewModelSource.includes("totalOutputTokenCount"),
  "station detail usage cards should render token input/output breakdown helpers",
);

assert(
  metricPanelSource.includes("min-h-[96px]") &&
    metricPanelSource.includes("h-9 w-9") &&
    metricPanelSource.includes("text-[22px]") &&
    metricPanelSource.includes("shadow-[0_2px_8px_rgba(15,23,42,0.08)]"),
  "metric panel cards should keep the wide statistic-card style from the reference",
);

assert(
  !metricPanelSource.includes('rounded-[var(--surface-radius)] bg-white p-3 shadow-[var(--surface-shadow)]'),
  "metric panel should not wrap statistic cards in a second white card",
);
