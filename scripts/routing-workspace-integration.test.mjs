import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();

function read(path) {
  return readFileSync(join(root, path), "utf8");
}

function assertIncludes(source, needle, label) {
  if (!source.includes(needle)) {
    throw new Error(`${label} should include ${needle}`);
  }
}

function assertExcludes(source, needle, label) {
  if (source.includes(needle)) {
    throw new Error(`${label} should not include ${needle}`);
  }
}

const routingPage = read("src/features/routing/RoutingPage.tsx");
const workspacePanel = read("src/features/routing/RoutingOperationalPreviewPanel.tsx");
const routingQueries = read("src/lib/queries/routingQueries.ts");
const routingTypes = read("src/lib/types/routing.ts");
const routingDeepLinks = read("src/lib/types/routingDeepLinks.ts");
const shellRegistry = read("src/app/shellPageRegistry.tsx");
const app = read("src/app/App.tsx");
const keyPoolPage = read("src/features/key-pool/KeyPoolPage.tsx");
const logsPage = read("src/features/logs/LogsPage.tsx");
const modelBasePricesPage = read("src/features/pricing/ModelBasePricesPage.tsx");
const channelStatusPage = read("src/features/channels/ChannelStatusPage.tsx");
const channelMonitoringTab = read("src/features/channels/ChannelMonitoringTab.tsx");
const collectorsPage = read("src/features/collectors/CollectorsPage.tsx");
const changeCenterPage = read("src/features/changes/ChangeCenterPage.tsx");
const stationsPage = read("src/features/stations/StationsPage.tsx");
const stationAssetRows = read("src/features/stations/pages/stations/StationAssetRows.tsx");
const stationDetailPage = read("src/features/stations/StationDetailPage.tsx");
const stationDetailContent = read("src/features/stations/components/StationDetailContent.tsx");

assertIncludes(routingPage, '{ value: "workspace", label: "工作台" }', "RoutingPage");
assertIncludes(routingPage, "refetchInterval: queryEnabled && activeTab === \"workspace\" ? 1_000 : false", "RoutingPage");
assertIncludes(routingPage, "routingQueryKeys.workspaceSnapshot({ limit: 50 })", "RoutingPage");
assertIncludes(routingPage, "routingQueryKeys.runtimeOverlay()", "RoutingPage");
assertIncludes(routingPage, "routingQueryKeys.recentDecisions({ limit: 8 })", "RoutingPage");
assertExcludes(routingPage, "loadRoutingWorkspace()", "RoutingPage");

assertIncludes(workspacePanel, "DataTableLite", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "getStationKeyOperationalDetailQuery", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "getRequestDecisionTraceQuery", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "simulateRouteQuery", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "snapshot-only", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "trace.timeline", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "timelineStatusTone", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "formatTimelineKind", "RoutingOperationalPreviewPanel");
assertExcludes(workspacePanel, "trace.planningRounds", "RoutingOperationalPreviewPanel");
assertExcludes(workspacePanel, "JSON.stringify", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "deepLink.kind === \"station-key\"", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "deepLink.kind === \"station\"", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "deepLink.kind === \"request\"", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "deepLink.kind === \"simulate-model\"", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "scopedCandidates", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "candidate.stationId === stationScopeId", "RoutingOperationalPreviewPanel");
assertExcludes(workspacePanel, "pricing_projector", "RoutingOperationalPreviewPanel");
assertExcludes(workspacePanel, "candidate_projector", "RoutingOperationalPreviewPanel");
assertExcludes(workspacePanel, "routing_engine", "RoutingOperationalPreviewPanel");

assertIncludes(routingQueries, 'runtimeOverlay: () => ["routing", "runtimeOverlay"] as const', "routingQueries");
assertIncludes(routingQueries, 'workspaceSnapshot: (input: RoutingWorkspaceSnapshotInput = {})', "routingQueries");

assertIncludes(routingTypes, 'from "@/lib/bridge/generated"', "routing types");
assertIncludes(routingTypes, "export type RoutingWorkspaceSnapshot = RoutingWorkspaceSnapshotDto", "routing types");
assertIncludes(routingTypes, "export type RouteSimulationResult = RouteSimulationResultDto", "routing types");
assertExcludes(routingTypes, "export type RoutingWorkspaceSnapshot = {", "routing types");
assertExcludes(routingTypes, "export type RouteSimulationResult = {", "routing types");
assertIncludes(routingDeepLinks, "export type RoutingDeepLink", "routing deep links");
assertIncludes(routingDeepLinks, 'kind: "station"', "routing deep links");
assertIncludes(routingDeepLinks, 'source?: "key_pool" | "monitoring" | "collector" | "station_endpoint_health" | "change_center"', "routing deep links");
assertIncludes(routingDeepLinks, 'source?: "request_log" | "change_center"', "routing deep links");

assertIncludes(shellRegistry, "openRoutingDeepLink", "shellPageRegistry");
assertIncludes(shellRegistry, "routingDeepLink", "shellPageRegistry");
assertIncludes(shellRegistry, "ChannelStatusPage onOpenRoutingDeepLink={actions.openRoutingDeepLink}", "shellPageRegistry");
assertIncludes(shellRegistry, "CollectorsPage onOpenRoutingDeepLink={actions.openRoutingDeepLink}", "shellPageRegistry");
assertIncludes(shellRegistry, "ChangeCenterPage onOpenRoutingDeepLink={actions.openRoutingDeepLink}", "shellPageRegistry");
assertIncludes(shellRegistry, "StationsPage", "shellPageRegistry");
assertIncludes(shellRegistry, "onOpenRoutingDeepLink={actions.openRoutingDeepLink}", "shellPageRegistry");
assertIncludes(app, "ModelBasePricesPage", "App");
assertIncludes(app, "onOpenRoutingDeepLink={openRoutingDeepLink}", "App");
assertIncludes(keyPoolPage, 'kind: "station-key"', "KeyPoolPage");
assertIncludes(logsPage, 'kind: "request"', "LogsPage");
assertIncludes(logsPage, "查看路由链路", "LogsPage");
assertIncludes(modelBasePricesPage, 'kind: "simulate-model"', "ModelBasePricesPage");
assertIncludes(modelBasePricesPage, 'source: "pricing"', "ModelBasePricesPage");
assertIncludes(modelBasePricesPage, "模拟", "ModelBasePricesPage");
assertIncludes(channelStatusPage, "ChannelMonitoringTab", "ChannelStatusPage");
assertIncludes(channelStatusPage, "onOpenRoutingDeepLink={onOpenRoutingDeepLink}", "ChannelStatusPage");
assertIncludes(channelMonitoringTab, 'source: "monitoring"', "ChannelMonitoringTab");
assertIncludes(channelMonitoringTab, "createMonitoringRoutingLink", "ChannelMonitoringTab");
assertIncludes(channelMonitoringTab, 'monitor.targetType !== "station_key"', "ChannelMonitoringTab");
assertIncludes(channelMonitoringTab, "路由影响", "ChannelMonitoringTab");
assertIncludes(collectorsPage, 'source: "collector"', "CollectorsPage");
assertIncludes(collectorsPage, 'kind: "station"', "CollectorsPage");
assertIncludes(collectorsPage, "查看路由影响", "CollectorsPage");
assertIncludes(changeCenterPage, "createChangeCenterRoutingLink", "ChangeCenterPage");
assertIncludes(changeCenterPage, 'source: "change_center"', "ChangeCenterPage");
assertIncludes(changeCenterPage, "event.requestLogId", "ChangeCenterPage");
assertIncludes(changeCenterPage, 'event.objectType === "station_key"', "ChangeCenterPage");
assertIncludes(changeCenterPage, 'event.objectType === "station"', "ChangeCenterPage");
assertIncludes(changeCenterPage, "查看路由影响", "ChangeCenterPage");
assertIncludes(stationsPage, "onOpenRoutingDeepLink", "StationsPage");
assertIncludes(stationAssetRows, 'source: "station_endpoint_health"', "StationAssetRows");
assertIncludes(stationAssetRows, 'kind: "station"', "StationAssetRows");
assertIncludes(stationAssetRows, "查看路由影响", "StationAssetRows");
assertIncludes(stationDetailPage, 'source: "station_endpoint_health"', "StationDetailPage");
assertIncludes(stationDetailContent, "查看路由影响", "StationDetailContent");

console.log("routing workspace integration contract ok");
