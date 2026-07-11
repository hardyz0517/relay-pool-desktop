import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");
const proxyApiSource = await readFile("src/lib/api/proxy.ts", "utf8");

assert.match(
  appShellSource,
  /import\s+\{[^}]*\bgetProxyStatus\b[^}]*\}\s+from\s+"@\/lib\/api\/proxy"/,
  "app shell should read the typed proxy status API",
);

assert.ok(
  appShellSource.includes("const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);"),
  "app shell should retain the most recently read proxy status",
);

assert.ok(
  appShellSource.includes("getProxyStatus().then(setProxyStatus).catch(() => {})") &&
    appShellSource.includes("setInterval(refreshProxyStatus, 2_000)") &&
    appShellSource.includes("clearInterval(intervalId)"),
  "app shell should read proxy status on mount and refresh it every two seconds",
);

assert.ok(
  proxyApiSource.includes('export const PROXY_STATUS_UPDATED_EVENT = "relay-pool:proxy-status-updated"') &&
    proxyApiSource.includes("window.dispatchEvent(") &&
    proxyApiSource.includes("new CustomEvent<ProxyStatus>(PROXY_STATUS_UPDATED_EVENT") &&
    proxyApiSource.match(/\.then\(publishProxyStatus\)/g)?.length === 3,
  "every successful proxy start, stop, or restart should immediately broadcast its returned status",
);

assert.ok(
  appShellSource.includes("PROXY_STATUS_UPDATED_EVENT") &&
    appShellSource.includes("window.addEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated)") &&
    appShellSource.includes("window.removeEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated)") &&
    appShellSource.includes("setProxyStatus((event as CustomEvent<ProxyStatus>).detail)"),
  "app shell should immediately apply proxy status broadcasts instead of waiting for the fallback poll",
);

assert.ok(
  appShellSource.includes('title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes('aria-label={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes("text-emerald-500") &&
    appShellSource.includes("text-amber-500"),
  "sidebar indicator should expose running and stopped labels with distinct green and amber dots",
);
