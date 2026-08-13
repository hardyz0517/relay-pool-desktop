import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const appSource = await readFile("src/app/App.tsx", "utf8");

const riskPosition = dashboardSource.indexOf("当前风险");
const healthPosition = dashboardSource.indexOf("密钥健康", riskPosition);
const queuePosition = dashboardSource.indexOf("路由队列", healthPosition);
const recentPosition = dashboardSource.indexOf("最近使用", queuePosition);

assert.ok(
  riskPosition >= 0 && riskPosition < healthPosition && healthPosition < queuePosition && queuePosition < recentPosition,
  "dashboard operational sections should read as risk, key health, then queue and recent usage",
);
assert.match(
  dashboardSource,
  /grid min-w-0 items-start gap-4 md:grid-cols-\[minmax\(0,3fr\)_minmax\(0,2fr\)\]/,
  "queue and recent usage should keep the 60/40 split at the minimum desktop window width",
);
assert.equal((dashboardSource.match(/查看全部/g) ?? []).length, 2);
assert.match(dashboardSource, /keyPoolItems\.slice\(0, 5\)/);
assert.match(dashboardSource, /requestLogs\.slice\(0, 5\)/);
assert.match(appSource, /const openRequestLogs = useCallback[\s\S]*?navigateTo\("logs"\)/);
assert.match(appSource, /const openLocalRouting = useCallback[\s\S]*?navigateTo\("routing"\)/);
assert.match(
  dashboardSource,
  /路由队列[\s\S]*?onOpenLocalRouting[\s\S]*?onClick=\{onOpenLocalRouting\}[\s\S]*?查看全部/,
  "route queue view-all should navigate to local routing",
);
