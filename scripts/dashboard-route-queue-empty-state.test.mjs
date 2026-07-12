import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  dashboardSource.includes('title="路由队列"'),
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
  dashboardSource.includes("`${key.priority + 1}`"),
  "dashboard route queue should display the same 1-based order users see in routing views",
);

assert(
  !dashboardSource.includes('label: "优先级"'),
  "dashboard route queue should not label the rendered order as 优先级",
);
