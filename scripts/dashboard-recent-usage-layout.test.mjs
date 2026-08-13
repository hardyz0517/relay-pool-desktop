import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const requestCostFormatSource = await readFile(
  "src/features/dashboard/requestCostFormat.ts",
  "utf8",
);

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  /<section className="grid min-w-0 gap-3">\s*<header[^>]*>[\s\S]*?最近使用[\s\S]*?查看全部/.test(dashboardSource),
  "dashboard recent usage should expose a compact title and view-all action",
);

assert(
  dashboardSource.includes("Inbox") &&
    dashboardSource.includes("暂无使用记录") &&
    dashboardSource.includes("开始使用 API 后，您的使用历史将显示在这里。"),
  "dashboard recent usage should explain the empty request-log state",
);

assert(
  /dashboardLoaded\s*&&\s*requestLogs\.length\s*===\s*0/.test(dashboardSource),
  "dashboard recent usage should render empty state only after a successful workspace load",
);

assert(
  /min-h-\[164px\][^\"]*items-center[^\"]*justify-center/.test(dashboardSource),
  "dashboard recent usage empty state should remain centered in the compact two-column area",
);

assert(
  !dashboardSource.includes("余额变化"),
  "dashboard recent usage section should not render balance change rows",
);

assert(
  dashboardSource.includes("FlaskConical") &&
    dashboardSource.includes("formatRecentRequestCost") &&
    dashboardSource.includes("formatTokenCount"),
  "dashboard request log rows should use the compact model/time + cost/token presentation",
);

assert(
  /requestLogs\.slice\(0,\s*5\)\.map/.test(dashboardSource),
  "dashboard recent usage should render at most five rows",
);

assert(
  requestCostFormatSource.includes('return "未定价";') &&
    requestCostFormatSource.includes('costStatus === "usage_only"') &&
    dashboardSource.includes("formatRecentRequestCost(request.estimatedTotalCost, request.costCurrency, request.costStatus)") &&
    !dashboardSource.includes("requestBaseCostValue(request)"),
  "dashboard request cost display should show usage-only rows as unpriced without a second base-cost column",
);

assert(
  !dashboardSource.includes('title="最近活动"') &&
    !dashboardSource.includes("requestStatusLabel(request.status)") &&
    !dashboardSource.includes('metrics={[{ label: "时间"'),
  "dashboard request log rows should drop the old status-badge/time-metric object-row layout",
);
