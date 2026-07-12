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
    /label:\s*"站点今日请求"[\s\S]*?valueClassName:\s*"text-slate-900"[\s\S]*?accent:\s*"green"/.test(dashboardSource) &&
    /label:\s*"站点今日消费"[\s\S]*?valueClassName:\s*"[^"]*text-purple-700[^"]*"[\s\S]*?accent:\s*"purple"/.test(dashboardSource) &&
    /label:\s*"站点今日 Token"[\s\S]*?valueClassName:\s*"text-slate-900"[\s\S]*?accent:\s*"amber"/.test(dashboardSource) &&
    /label:\s*"站点累计 Token"[\s\S]*?valueClassName:\s*"text-slate-900"[\s\S]*?accent:\s*"indigo"/.test(dashboardSource),
  "dashboard station usage metric cards should align their primary value colors with the matching local routing metric cards",
);

assert(
  /label:\s*"站点今日请求"[\s\S]*?detail:\s*`总计：\$\{formatCompactNumber\(stationUsage\.totalRequestCount\)\}`/.test(dashboardSource),
  "dashboard station request card should label the cumulative request count as 总计：",
);

assert(
  dashboardSource.includes("stationUsage.todayBaseConsumption") &&
    dashboardSource.includes("stationUsage.totalBaseConsumption") &&
    dashboardSource.includes('formatUsdAmount(stationUsage.todayBaseConsumption)') &&
    dashboardSource.includes('formatUsdAmount(stationUsage.totalBaseConsumption)') &&
    dashboardSource.includes("总计："),
  "dashboard station consumption card should render actual/base consumption totals with a 总计： detail",
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
  /label:\s*"今日请求"[\s\S]*?helper:\s*`总计：\$\{formatUsageCount\(currentBalance\.sourceSnapshot\?\.totalRequestCount\)\}`/.test(stationDetailViewModelSource),
  "station detail request usage card should label the cumulative request count as 总计：",
);

assert(
  /label:\s*"今日消费"[\s\S]*?helper:\s*`总计：\$\{formatUsageMoney\(currentBalance\.sourceSnapshot\?\.totalConsumption\)\}`/.test(stationDetailViewModelSource),
  "station detail consumption usage card should label the cumulative consumption as 总计：",
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
