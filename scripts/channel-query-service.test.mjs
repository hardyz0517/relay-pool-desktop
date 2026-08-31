import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const querySource = await readFile("src/lib/queries/channelQueries.ts", "utf8");
const monitoringSource = await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8");
const statusPageSource = await readFile("src/features/channels/ChannelStatusPage.tsx", "utf8");
const statusSource = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");

assert.ok(
  querySource.includes("export type { ChannelMonitoringWorkspace, ChannelStatusWorkspace }") &&
    querySource.includes('from "@/lib/bridge/BackendClient"'),
  "channel query service should expose monitoring raw facts workspace shape",
);

assert.ok(
  querySource.includes("export type { ChannelMonitoringWorkspace, ChannelStatusWorkspace }") &&
    querySource.includes('from "@/lib/bridge/BackendClient"'),
  "channel query service should expose status backend summary workspace shape",
);

assert.ok(
  querySource.includes("export function loadChannelMonitoringWorkspace()") &&
    querySource.includes("getActiveBackendClient().channels.loadChannelMonitoringWorkspace()"),
  "channel query service should delegate monitoring workspace reads to the active backend client",
);

assert.ok(
  querySource.includes("export function loadChannelStatusWorkspace()") &&
    querySource.includes("getActiveBackendClient().channels.loadChannelStatusWorkspace()"),
  "channel query service should delegate status workspace reads to the active backend client",
);

assert.ok(
    monitoringSource.includes("useActivityQuery(channelMonitoringQueryOptions())") &&
    monitoringSource.includes("const workspace = workspaceQuery.data") &&
    monitoringSource.includes("const monitors = workspace?.monitors ?? []") &&
    monitoringSource.includes("workspace?.statusWorkspace.rows ?? []") &&
    monitoringSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitoring })"),
  "channel monitoring tab should consume the canonical monitoring workspace query cache",
);

assert.ok(
  !querySource.includes("filterLogsByWindow") &&
    !querySource.includes("buildChannels") &&
    !querySource.includes("orderChannelsBySavedOrder") &&
    !querySource.includes("runChannelMonitorNow") &&
    !querySource.includes("createChannelMonitor") &&
    !querySource.includes("updateChannelMonitor") &&
    !querySource.includes("deleteChannelMonitor"),
  "channel query service must not define channel view behavior or write actions",
);

assert.ok(
  !monitoringSource.includes('import { loadChannelMonitoringWorkspace } from "@/lib/queries/channelQueries";') &&
    !monitoringSource.includes("const workspace = await loadChannelMonitoringWorkspace()") &&
    !monitoringSource.includes("setMonitors") &&
    !monitoringSource.includes("setStations") &&
    !monitoringSource.includes("setKeys") &&
    !monitoringSource.includes("setTemplates") &&
    !monitoringSource.includes("usePageActivation"),
  "channel monitoring tab should not keep local workspace server-state or activation loader",
);

assert.ok(
  statusSource.includes("useChannelStatusController()") &&
    statusSource.includes("controller.workspaceView.rows") &&
    !statusSource.includes('import { loadChannelStatusWorkspace } from "@/lib/queries/channelQueries";') &&
    !statusSource.includes("setKeys(workspace.keyPoolItems)") &&
    !statusSource.includes("setLogs(workspace.requestLogs)") &&
    !statusSource.includes("setHealth(workspace.stationKeyHealth)") &&
    !statusSource.includes("setStatusSummaries(workspace.channelStatusSummaries)"),
  "channel status tab should consume the canonical status workspace query cache",
);

assert.ok(
  statusPageSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.channelStatus })") &&
    !statusPageSource.includes("statusRefreshToken") &&
    !statusSource.includes("refreshToken") &&
    !statusSource.includes("useEffect") &&
    statusSource.includes("statusQuery.refetch({ throwOnError: true })"),
  "channel status refresh should stay inside the canonical status query owner",
);

assert.ok(
    !monitoringSource.includes("listChannelMonitorSummaries") &&
    !/Promise\.all\(\[\s*listChannelMonitorSummaries\(\),\s*listStations\(\),\s*listKeyPoolItems\(\),\s*listChannelMonitorTemplates\(\),?\s*\]\)/s.test(
    monitoringSource,
  ),
  "channel tabs should no longer own initial raw fact Promise.all orchestration",
);
