import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  /<section className="grid gap-3">\s*<h2 className="truncate text-\[13px\] font-semibold text-slate-800">\s*路由队列\s*<\/h2>/.test(dashboardSource),
  "dashboard should keep the route queue section on the home page",
);

assert(
  /dashboardLoaded\s*&&\s*keyPoolItems\.length\s*===\s*0/.test(dashboardSource),
  "dashboard route queue should render an empty state only after a successful workspace load",
);

assert(
  dashboardSource.includes("暂无路由队列") &&
    dashboardSource.includes("添加或导入 Key 后，可用路由将显示在这里。"),
  "dashboard route queue empty state should explain why the queue is empty",
);

assert(
  /keyPoolItems\.slice\(0,\s*6\)\.map/.test(dashboardSource),
  "dashboard route queue should keep rendering existing key rows when keys are available",
);

assert(
  dashboardSource.includes('label: "顺位"'),
  "dashboard route queue should call the visible order 顺位 instead of exposing internal priority",
);

assert(
  /keyPoolItems\.slice\(0,\s*6\)\.map\(\(\s*key\s*,\s*index\s*\)\s*=>/.test(dashboardSource),
  "dashboard route queue should derive the visible order from the rendered queue index",
);

assert(
  dashboardSource.includes('label: "顺位", value: `${index + 1}`') &&
    !dashboardSource.includes("`${key.priority + 1}`"),
  "dashboard route queue should not expose duplicate per-station priority values as global order",
);

assert(
  !dashboardSource.includes('label: "优先级"'),
  "dashboard route queue should not label the rendered order as 优先级",
);
