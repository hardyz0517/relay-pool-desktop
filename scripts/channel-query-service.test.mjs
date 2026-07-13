import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const querySource = await readFile("src/lib/queries/channelQueries.ts", "utf8");
const monitoringSource = await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8");
const statusSource = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");

assert.ok(
  querySource.includes("export type ChannelMonitoringWorkspace") &&
    querySource.includes("monitorSummaries: ChannelMonitorSummary[]") &&
    querySource.includes("stations: Station[]") &&
    querySource.includes("keyPoolItems: KeyPoolItem[]") &&
    querySource.includes("templates: ChannelMonitorRequestTemplate[]"),
  "channel query service should expose monitoring raw facts workspace shape",
);

assert.ok(
  querySource.includes("export type ChannelStatusWorkspace") &&
    querySource.includes("requestLogs: RequestLog[]") &&
    querySource.includes("stationKeyHealth: StationKeyHealth[]") &&
    querySource.includes("channelStatusSummaries: ChannelStatusSummary[]"),
  "channel query service should expose status backend summary workspace shape",
);

assert.ok(
  querySource.includes("export async function loadChannelMonitoringWorkspace()") &&
    querySource.includes("listChannelMonitorSummaries()") &&
    querySource.includes("listStations()") &&
    querySource.includes("listKeyPoolItems()") &&
    querySource.includes("listChannelMonitorTemplates()"),
  "channel query service should orchestrate monitoring raw fact reads",
);

assert.ok(
  querySource.includes("export async function loadChannelStatusWorkspace()") &&
    querySource.includes("listRequestLogs()") &&
    querySource.includes("listStationKeyHealth()") &&
    querySource.includes("listChannelStatusSummaries()"),
  "channel query service should orchestrate status summary reads",
);

const monitoringWorkspaceSource = querySource.slice(
  querySource.indexOf("export async function loadChannelMonitoringWorkspace()"),
  querySource.indexOf("export async function loadChannelStatusWorkspace()"),
);
assert.ok(
  monitoringSource.includes("const workspace = await loadChannelMonitoringWorkspace()") &&
    monitoringWorkspaceSource.includes("listChannelMonitorSummaries()") &&
    !monitoringWorkspaceSource.includes("runLimit") &&
    !monitoringWorkspaceSource.includes("runSince"),
  "channel monitoring tab should keep the default lightweight recent monitor summaries",
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
  monitoringSource.includes('import { loadChannelMonitoringWorkspace } from "@/lib/queries/channelQueries";') &&
    monitoringSource.includes("const workspace = await loadChannelMonitoringWorkspace()") &&
    monitoringSource.includes("const nextMonitors = workspace.monitorSummaries.map((summary) => summary.monitor)") &&
    monitoringSource.includes("setStations(workspace.stations)") &&
    monitoringSource.includes("setKeys(workspace.keyPoolItems)") &&
    monitoringSource.includes("setTemplates(workspace.templates)"),
  "channel monitoring tab should consume the monitoring query service without changing state assignments",
);

assert.ok(
  statusSource.includes('import { loadChannelStatusWorkspace } from "@/lib/queries/channelQueries";') &&
    statusSource.includes("const workspace = await loadChannelStatusWorkspace()") &&
    statusSource.includes("const nextMonitors = workspace.channelStatusSummaries.map((summary) => summary.monitor)") &&
    statusSource.includes("setKeys(workspace.keyPoolItems)") &&
    statusSource.includes("setLogs(workspace.requestLogs)") &&
    statusSource.includes("setHealth(workspace.stationKeyHealth)") &&
    statusSource.includes("setStatusSummaries(workspace.channelStatusSummaries)"),
  "channel status tab should consume the status query service without changing state assignments",
);

assert.ok(
  !/Promise\.all\(\[\s*listChannelMonitorSummaries\(\),\s*listStations\(\),\s*listKeyPoolItems\(\),\s*listChannelMonitorTemplates\(\),?\s*\]\)/s.test(
    monitoringSource,
  ) &&
    !/Promise\.all\(\[\s*listKeyPoolItems\(\),\s*listRequestLogs\(\),\s*listStationKeyHealth\(\),\s*listChannelMonitorSummaries\(\),?\s*\]\)/s.test(
      statusSource,
    ),
  "channel tabs should no longer own initial raw fact Promise.all orchestration",
);
