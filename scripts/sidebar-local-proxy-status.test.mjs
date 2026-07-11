import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");

assert.match(
  appShellSource,
  /import\s+\{\s*getProxyStatus\s*\}\s+from\s+"@\/lib\/api\/proxy"/,
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
  appShellSource.includes('title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes('aria-label={proxyRunning ? "本地代理运行中" : "本地代理未启动"}') &&
    appShellSource.includes("text-emerald-500") &&
    appShellSource.includes("text-amber-500"),
  "sidebar indicator should expose running and stopped labels with distinct green and amber dots",
);
