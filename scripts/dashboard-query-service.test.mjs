import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const querySource = await readFile("src/lib/queries/dashboardQueries.ts", "utf8");
const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.ok(
  querySource.includes("export type DashboardWorkspace") &&
    querySource.includes("proxyStatus: ProxyStatus") &&
    querySource.includes("requestLogs: RequestLog[]") &&
    querySource.includes("keyPoolItems: KeyPoolItem[]") &&
    querySource.includes("balanceSnapshots: BalanceSnapshot[]") &&
    querySource.includes("settings: AppSettings") &&
    querySource.includes("changeEvents: ChangeEvent[]"),
  "dashboard query service should expose a raw facts workspace shape",
);

assert.ok(
  querySource.includes("export async function loadDashboardWorkspace()") &&
    querySource.includes("getProxyStatus()") &&
    querySource.includes("listRequestLogs()") &&
    querySource.includes("listKeyPoolItems()") &&
    querySource.includes("listBalanceSnapshots()") &&
    querySource.includes("getSettings()") &&
    querySource.includes("listChangeEvents()"),
  "dashboard query service should orchestrate existing raw fact reads",
);

assert.ok(
  !querySource.includes("summarizeDashboardBalances") &&
    !querySource.includes("unreadRiskCount") &&
    !querySource.includes("getLocalAccessKey"),
  "dashboard query service must not define dashboard business projections or eagerly read full local secrets",
);

assert.ok(
  dashboardSource.includes("loadDashboardWorkspace()") &&
    dashboardSource.includes("setProxyStatus(workspace.proxyStatus)") &&
    dashboardSource.includes("setRequestLogs(workspace.requestLogs)") &&
    dashboardSource.includes("setKeyPoolItems(workspace.keyPoolItems)") &&
    dashboardSource.includes("setBalanceSnapshots(workspace.balanceSnapshots)") &&
    dashboardSource.includes("setSettings(workspace.settings)") &&
    dashboardSource.includes("setChangeEvents(workspace.changeEvents)"),
  "dashboard should consume the query service without changing existing state assignments",
);

assert.ok(
  !/void\s+Promise\.all\(\[\s*getProxyStatus\(\),\s*listRequestLogs\(\),\s*listKeyPoolItems\(\),\s*listBalanceSnapshots\(\),\s*getSettings\(\),\s*listChangeEvents\(\),?\s*\]\)/s.test(
    dashboardSource,
  ),
  "dashboard page should no longer own the initial raw fact Promise.all orchestration",
);
