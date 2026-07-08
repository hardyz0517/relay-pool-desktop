import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

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
  dashboardSource.includes('return "未定价";') &&
    dashboardSource.includes("pricedRequestLogs") &&
    dashboardSource.includes('costStatus === "usage_only"') &&
    dashboardSource.includes("request.costStatus"),
  "dashboard request cost display should show usage-only rows as unpriced instead of $0.0000",
);

assert(
  !dashboardSource.includes('title="最近活动"') &&
    !dashboardSource.includes("requestStatusLabel(request.status)") &&
    !dashboardSource.includes('metrics={[{ label: "时间"'),
  "dashboard request log rows should drop the old status-badge/time-metric object-row layout",
);
