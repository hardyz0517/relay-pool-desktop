import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.match(
  dashboardSource,
  /import\s+\{\s*getProxyStatus,\s*listRequestLogs,\s*startLocalProxy,\s*stopLocalProxy\s*\}\s+from\s+"@\/lib\/api\/proxy"/,
  "dashboard should use the typed proxy API boundary when starting and stopping the local route",
);

assert.ok(
  dashboardSource.includes("const [startingLocalProxy, setStartingLocalProxy] = useState(false);"),
  "dashboard should track a dedicated starting state for the local route action",
);

assert.ok(
  dashboardSource.includes("const [stoppingLocalProxy, setStoppingLocalProxy] = useState(false);"),
  "dashboard should track a dedicated stopping state for the local route action",
);

assert.ok(
  dashboardSource.includes("async function handleStartLocalProxy()") &&
    dashboardSource.includes("setStartingLocalProxy(true)") &&
    dashboardSource.includes("const nextStatus = await startLocalProxy()") &&
    dashboardSource.includes("setProxyStatus(nextStatus)") &&
    dashboardSource.includes("setStartingLocalProxy(false)"),
  "dashboard start handler should start the local route and refresh the displayed proxy status",
);

assert.ok(
  dashboardSource.includes("async function handleStopLocalProxy()") &&
    dashboardSource.includes("setStoppingLocalProxy(true)") &&
    dashboardSource.includes("const nextStatus = await stopLocalProxy()") &&
    dashboardSource.includes("setProxyStatus(nextStatus)") &&
    dashboardSource.includes("setStoppingLocalProxy(false)"),
  "dashboard stop handler should stop the local route and refresh the displayed proxy status",
);

assert.ok(
  dashboardSource.includes('toast.success("本地路由已启动"') &&
    dashboardSource.includes('toast.error("启动本地路由失败"') &&
    dashboardSource.includes('toast.success("本地路由已关闭"') &&
    dashboardSource.includes('toast.error("关闭本地路由失败"'),
  "dashboard should surface success and failure feedback for local route startup and shutdown",
);

assert.ok(
  dashboardSource.includes("disabled={startingLocalProxy || stoppingLocalProxy}") &&
    dashboardSource.includes('aria-label={proxyRunning ? "关闭本地路由" : "启动本地路由"}') &&
    dashboardSource.includes("Power") &&
    dashboardSource.includes("bg-[#0060DF]") &&
    dashboardSource.includes("bg-[#EFF0F3]"),
  "dashboard should render a compact start/stop local route action with the requested blue and gray states",
);
