import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");
const proxyApiSource = await readFile("src/lib/api/proxy.ts", "utf8");

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
  proxyApiSource.includes('export const PROXY_STATUS_UPDATED_EVENT = "relay-pool:proxy-status-updated"') &&
    proxyApiSource.includes("window.dispatchEvent(") &&
    proxyApiSource.includes("new CustomEvent<ProxyStatus>(PROXY_STATUS_UPDATED_EVENT") &&
    (proxyApiSource.match(/\.then\(publishProxyStatus\)/g)?.length ?? 0) >= 4,
  "every successful proxy lifecycle action should immediately broadcast its returned status",
);

assert.ok(
  appShellSource.includes("PROXY_STATUS_UPDATED_EVENT") &&
    appShellSource.includes("window.addEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated)") &&
    appShellSource.includes("window.removeEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated)") &&
    appShellSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, (event as CustomEvent<ProxyStatus>).detail)"),
  "app shell should immediately apply proxy status broadcasts instead of waiting for the fallback poll",
);

assert.ok(
  appShellSource.includes('title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes('aria-label={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes("<LocalProxyRadarIcon") &&
    appShellSource.includes("active={proxyRunning}") &&
    appShellSource.includes('proxyRunning ? "text-success-foreground" : "text-muted-foreground"'),
  "sidebar indicator should expose running and stopped labels with distinct radar states",
);
