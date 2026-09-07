import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  /<section className="grid min-w-0 gap-3"[^>]*>\s*<header[^>]*>[\s\S]*?路由队列[\s\S]*?查看全部/.test(dashboardSource),
  "dashboard should keep the route queue section on the home page",
);

assert(
  /dashboardLoaded\s*&&\s*dashboardRoutingItems\.length\s*===\s*0/.test(dashboardSource),
  "dashboard route queue should render an empty state only after a successful workspace load",
);

assert(
  dashboardSource.includes("暂无路由队列") &&
    dashboardSource.includes("添加或导入密钥后，可用路由将显示在这里。"),
  "dashboard route queue empty state should explain why the queue is empty",
);

assert(
  /dashboardRoutingItems\.slice\(0,\s*5\)\.map/.test(dashboardSource),
  "dashboard route queue should render at most five key rows",
);

assert(
  dashboardSource.includes('label: "当前并发"'),
  "dashboard route queue should show current concurrency instead of the visible row order",
);

assert(
  dashboardSource.includes("routingQueryKeys.runtimeOverlay()") &&
    dashboardSource.includes("loadRoutingRuntimeOverlayQuery") &&
    dashboardSource.includes("candidate.stationKeyInFlight"),
  "dashboard route queue should read canonical per-key runtime concurrency",
);

assert(
  /label:\s*"当前并发"[\s\S]*?StatusBadge[\s\S]*?tone=\{currentConcurrency != null && currentConcurrency > 0 \? "healthy" : "disabled"\}[\s\S]*?className="rounded-\[4px\]"[\s\S]*?currentConcurrency == null \? "—" : String\(currentConcurrency\)[\s\S]*?align:\s*"center"/.test(dashboardSource),
  "dashboard route queue should highlight active concurrency with the routing-page StatusBadge",
);

assert(
  !dashboardSource.includes('label: "顺位"') &&
    !dashboardSource.includes('label: "优先级"'),
  "dashboard route queue should not expose row order as an operational metric",
);
