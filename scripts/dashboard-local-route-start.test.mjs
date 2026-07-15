import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

assert.match(
  dashboardSource,
  /import\s+\{[^}]*startLocalProxy[^}]*stopLocalProxy[^}]*\}\s+from\s+"@\/lib\/api\/proxy"/,
  "dashboard should use the typed proxy API boundary when starting and stopping the local route",
);

assert.ok(
  dashboardSource.includes('import { useActivityQuery } from "@/lib/query/useActivityQuery";') &&
    dashboardSource.includes("proxyStatusQueryOptions") &&
    dashboardSource.includes("requestLogsQueryOptions") &&
    dashboardSource.includes("keyPoolQueryOptions") &&
    dashboardSource.includes("stationsQueryOptions") &&
    dashboardSource.includes("currentStationBalanceSnapshotsQueryOptions") &&
    dashboardSource.includes("settingsQueryOptions") &&
    dashboardSource.includes("changeEventsQueryOptions"),
  "dashboard initial raw facts should load through shared resource query options",
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
    dashboardSource.includes("await queryClient.cancelQueries({ queryKey: queryKeys.proxyStatus })") &&
    dashboardSource.includes("const nextStatus = await startLocalProxy()") &&
    dashboardSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, nextStatus)") &&
    dashboardSource.includes("setStartingLocalProxy(false)"),
  "dashboard start handler should start the local route and refresh the displayed proxy status",
);

assert.ok(
  dashboardSource.includes("async function handleStopLocalProxy()") &&
    dashboardSource.includes("setStoppingLocalProxy(true)") &&
    dashboardSource.includes("await queryClient.cancelQueries({ queryKey: queryKeys.proxyStatus })") &&
    dashboardSource.includes("const nextStatus = await stopLocalProxy()") &&
    dashboardSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, nextStatus)") &&
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
    dashboardSource.includes("bg-primary-solid") &&
    dashboardSource.includes("bg-surface"),
  "dashboard should render a compact start/stop local route action with the requested blue and gray states",
);
