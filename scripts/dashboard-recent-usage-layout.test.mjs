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
  /<SectionCard\s+title="最近使用"[\s>]/.test(dashboardSource),
  "dashboard recent activity section should be renamed to 最近使用",
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
  /min-h-\[260px\][^\"]*items-center[^\"]*justify-center/.test(dashboardSource),
  "dashboard recent usage empty state should remain centered in a stable content area",
);

assert(
  !dashboardSource.includes("余额变化"),
  "dashboard recent usage section should not render balance change rows",
);

assert(
  dashboardSource.includes("FlaskConical") &&
    dashboardSource.includes("formatRequestCost") &&
    dashboardSource.includes("formatTokenCount"),
  "dashboard request log rows should use the compact model/time + cost/token presentation",
);

assert(
  requestCostFormatSource.includes('return "未定价";') &&
    requestCostFormatSource.includes("request.baseTotalCost") &&
    dashboardSource.includes("requestBaseCostValue(request)") &&
    requestCostFormatSource.includes('costStatus === "usage_only"') &&
    dashboardSource.includes("request.costStatus"),
  "dashboard request cost display should show usage-only rows as unpriced instead of $0.0000",
);

assert(
  !dashboardSource.includes('title="最近活动"') &&
    !dashboardSource.includes("requestStatusLabel(request.status)") &&
    !dashboardSource.includes('metrics={[{ label: "时间"'),
  "dashboard request log rows should drop the old status-badge/time-metric object-row layout",
);
