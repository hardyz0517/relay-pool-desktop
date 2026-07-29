import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");
const proxyApiSource = await readFile("src/lib/api/proxy.ts", "utf8");
const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const routingSource = await readFile("src/features/routing/RoutingPage.tsx", "utf8");
const settingsSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const updaterSource = await readFile("src/lib/updater/UpdaterProvider.tsx", "utf8");

assert.match(
  appShellSource,
  /proxyStatusQueryOptions/,
  "app shell should read proxy status through the shared query options",
);

assert.ok(
  appShellSource.includes("const { data: proxyStatus = null } = useQuery(proxyStatusQueryOptions(2_000));"),
  "app shell should retain the most recently read proxy status in the shared query cache",
);

assert.ok(
  appShellSource.includes("useQuery(proxyStatusQueryOptions(2_000))"),
  "app shell should read proxy status on mount and refresh it every two seconds through React Query",
);

assert.ok(
  !proxyApiSource.includes("PROXY_STATUS_UPDATED_EVENT") &&
    !proxyApiSource.includes("relay-pool:proxy-status-updated") &&
    !proxyApiSource.includes("window.dispatchEvent(") &&
    !proxyApiSource.includes("CustomEvent<ProxyStatus>") &&
    !proxyApiSource.includes("publishProxyStatus"),
  "proxy API should not expose DOM status synchronization events",
);

assert.ok(
  !appShellSource.includes("PROXY_STATUS_UPDATED_EVENT") &&
    !appShellSource.includes("handleProxyStatusUpdated") &&
    !appShellSource.includes("CustomEvent<ProxyStatus>"),
  "app shell should not subscribe to proxy DOM synchronization",
);

assert.ok(
  dashboardSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, nextStatus)") &&
    routingSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, nextStatus)") &&
    settingsSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, nextStatus)") &&
    updaterSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, nextProxyStatus)"),
  "proxy lifecycle mutation owners should write returned statuses into the shared query cache",
);

assert.ok(
  appShellSource.includes('title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes('aria-label={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes("<LocalProxyRadarIcon") &&
    appShellSource.includes("active={proxyRunning}") &&
    appShellSource.includes('proxyRunning ? "text-success-foreground" : "text-muted-foreground"'),
  "sidebar indicator should expose running and stopped labels with distinct radar states",
);
