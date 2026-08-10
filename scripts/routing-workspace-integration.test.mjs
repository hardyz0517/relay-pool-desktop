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
const diagnosticsPanel = read("src/features/routing/RoutingStatusDiagnosticsPanel.tsx");
const localRoutingStatusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const routingQueries = read("src/lib/queries/routingQueries.ts");
const routingSynchronization = read("src/lib/query/routingQuerySynchronization.ts");
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
const manualTauriScript = read("scripts/run-routing-workspace-tauri-manual.ps1");
const fixtureServer = read("scripts/routing-workspace-fixture-server.mjs");
const tauriCdpVerifier = read("scripts/verify-routing-workspace-tauri-cdp.mjs");

// Keep the workspace contract checks aligned with the current query-owned surface.
assertIncludes(routingPage, 'type LocalRoutingTab = "status" | "edit"', "RoutingPage");
assertIncludes(routingPage, "RoutingStatusDiagnosticsPanel", "RoutingPage");
assertIncludes(routingPage, "refetchInterval: queryEnabled && activeTab === \"status\" ? 1_000 : false", "RoutingPage");
assertIncludes(routingPage, "routingQueryKeys.workspaceSnapshot({ limit: 50 })", "RoutingPage");
assertIncludes(routingPage, "routingQueryKeys.runtimeOverlay()", "RoutingPage");
assertIncludes(routingPage, "routingQueryKeys.recentDecisions({ limit: 8 })", "RoutingPage");
assertIncludes(routingPage, "maxRateMultiplier={routingSnapshotQuery.data?.maxRateMultiplier}", "RoutingPage");
assertIncludes(routingPage, "deepLink={deepLink}", "RoutingPage");
assertExcludes(routingPage, 'value: "workspace"', "RoutingPage");
assertExcludes(routingPage, "RoutingOperationalPreviewPanel", "RoutingPage");
assertExcludes(routingPage, "loadRoutingWorkspace()", "RoutingPage");
assertExcludes(routingPage, "cancelQueries", "RoutingPage");
assertExcludes(routingPage, "removeQueries", "RoutingPage");
assertExcludes(routingPage, "resetQueries", "RoutingPage");
assertExcludes(routingPage, "setQueryData(routingQueryKeys.runtimeOverlay()", "RoutingPage");
assertIncludes(routingPage, "refreshRoutingQueries(queryClient)", "RoutingPage");
assertExcludes(routingSynchronization, "queryKeys.localRoutingWorkspace", "routing query synchronization");
assertIncludes(routingSynchronization, "queryClient.invalidateQueries({ queryKey: routingQueryKeys.all })", "routing query synchronization");
assertIncludes(routingSynchronization, "synchronizeRoutingQueriesAfterMutation", "routing query synchronization");

assertIncludes(localRoutingStatusTab, "simulateRouteQuery", "LocalRoutingStatusTab");
assertIncludes(localRoutingStatusTab, "deepLink?.kind !== \"simulate-model\"", "LocalRoutingStatusTab");
assertIncludes(diagnosticsPanel, "runtimeOverlay?.candidates", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "decisions?.decisions", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "onOpenRequestLog", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "deepLink.kind === \"station-key\"", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "deepLink.kind === \"station\"", "RoutingStatusDiagnosticsPanel");
assertIncludes(diagnosticsPanel, "candidate.stationId === stationScopeId", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "pricing_projector", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "candidate_projector", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "routing_engine", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "JSON.stringify", "RoutingStatusDiagnosticsPanel");
assertExcludes(diagnosticsPanel, "DataTableLite", "RoutingStatusDiagnosticsPanel");

assertIncludes(routingQueries, 'runtimeOverlay: () => ["routing", "runtimeOverlay"] as const', "routingQueries");
assertIncludes(routingQueries, 'all: ["routing"] as const', "routingQueries");
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
assertIncludes(shellRegistry, "requestLogDeepLink", "shellPageRegistry");
assertIncludes(shellRegistry, "openRequestLogDeepLink", "shellPageRegistry");
assertIncludes(shellRegistry, "ChannelStatusPage onOpenRoutingDeepLink={routingDeepLinkHandler}", "shellPageRegistry");
assertIncludes(shellRegistry, "CollectorsPage onOpenRoutingDeepLink={routingDeepLinkHandler}", "shellPageRegistry");
assertIncludes(shellRegistry, "ChangeCenterPage", "shellPageRegistry");
assertIncludes(shellRegistry, "StationsPage", "shellPageRegistry");
assertIncludes(shellRegistry, "onOpenRoutingDeepLink={routingDeepLinkHandler}", "shellPageRegistry");
assertIncludes(app, "ModelBasePricesPage", "App");
assertIncludes(app, "onOpenRoutingDeepLink={developerModeEnabled ? openRoutingDeepLink : undefined}", "App");
assertIncludes(app, "requestLogDeepLinkSequenceRef", "App");
assertIncludes(app, "navigateTo(\"logs\")", "App");
assertIncludes(keyPoolPage, 'kind: "station-key"', "KeyPoolPage");
assertIncludes(logsPage, 'kind: "request"', "LogsPage");
assertIncludes(logsPage, "VersionedRequestLogDeepLink", "LogsPage");
assertIncludes(logsPage, "deepLink.requestLogId", "LogsPage");
assertIncludes(logsPage, "setSelectedId(deepLink.requestLogId)", "LogsPage");
assertIncludes(modelBasePricesPage, 'kind: "simulate-model"', "ModelBasePricesPage");
assertIncludes(modelBasePricesPage, 'source: "pricing"', "ModelBasePricesPage");
assertIncludes(channelStatusPage, "ChannelMonitoringTab", "ChannelStatusPage");
assertIncludes(channelStatusPage, "onOpenRoutingDeepLink={onOpenRoutingDeepLink}", "ChannelStatusPage");
assertIncludes(channelMonitoringTab, 'source: "monitoring"', "ChannelMonitoringTab");
assertIncludes(channelMonitoringTab, "createMonitoringRoutingLink", "ChannelMonitoringTab");
assertIncludes(channelMonitoringTab, 'monitor.targetType !== "station_key"', "ChannelMonitoringTab");
assertIncludes(collectorsPage, 'source: "collector"', "CollectorsPage");
assertIncludes(collectorsPage, 'kind: "station"', "CollectorsPage");
assertIncludes(changeCenterPage, "createChangeCenterRoutingLink", "ChangeCenterPage");
assertIncludes(changeCenterPage, 'source: "change_center"', "ChangeCenterPage");
assertIncludes(changeCenterPage, "event.requestLogId", "ChangeCenterPage");
assertIncludes(changeCenterPage, 'event.objectType === "station_key"', "ChangeCenterPage");
assertIncludes(changeCenterPage, 'event.objectType === "station"', "ChangeCenterPage");
assertIncludes(stationsPage, "onOpenRoutingDeepLink", "StationsPage");
assertIncludes(stationAssetRows, 'source: "station_endpoint_health"', "StationAssetRows");
assertIncludes(stationAssetRows, 'kind: "station"', "StationAssetRows");
assertIncludes(stationDetailPage, 'source: "station_endpoint_health"', "StationDetailPage");
assertIncludes(stationDetailContent, "onOpenRoutingDeepLink", "StationDetailContent");

assertIncludes(manualTauriScript, "output\\manual-routing-workspace\\$ProfileName", "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, 'dev.relaypool.desktop.routing-workspace-manual', "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, "tauri-dev-overlay.json", "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, "beforeDevCommand", "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, "pnpm dev --port $DevServerPort --strictPort", "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, "$env:APPDATA = $appData", "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, "$env:LOCALAPPDATA = $localAppData", "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, '$env:RELAY_POOL_DEV_AUTO_START_PROXY = "0"', "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, '$env:RELAY_POOL_START_PROXY_ON_LAUNCH = "0"', "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "manual Tauri routing workspace script");
assertIncludes(manualTauriScript, "pnpm.cmd tauri dev --config $overlayPath", "manual Tauri routing workspace script");
assertExcludes(manualTauriScript, "Remove-Item", "manual Tauri routing workspace script");
assertExcludes(manualTauriScript, "rm -", "manual Tauri routing workspace script");
assertExcludes(manualTauriScript, "git add", "manual Tauri routing workspace script");
assertIncludes(fixtureServer, 'const host = "127.0.0.1"', "routing workspace fixture server");
assertIncludes(fixtureServer, 'url.pathname === "/v1/models"', "routing workspace fixture server");
assertIncludes(fixtureServer, 'url.pathname === "/v1/chat/completions"', "routing workspace fixture server");
assertIncludes(fixtureServer, 'url.pathname === "/v1/responses"', "routing workspace fixture server");
assertIncludes(fixtureServer, 'url.pathname === "/v1/embeddings"', "routing workspace fixture server");
assertIncludes(fixtureServer, "This server does not log request bodies or headers", "routing workspace fixture server");
assertExcludes(fixtureServer, "console.log(request", "routing workspace fixture server");
assertExcludes(fixtureServer, "console.log(body", "routing workspace fixture server");
assertExcludes(fixtureServer, "Authorization", "routing workspace fixture server");
assertExcludes(fixtureServer, "https://", "routing workspace fixture server");
assertIncludes(tauriCdpVerifier, "output\", \"manual-routing-workspace", "routing workspace Tauri CDP verifier");
assertIncludes(tauriCdpVerifier, "dev.relaypool.desktop.routing-workspace-cdp", "routing workspace Tauri CDP verifier");
assertIncludes(tauriCdpVerifier, "scripts/routing-workspace-fixture-server.mjs", "routing workspace Tauri CDP verifier");
assertIncludes(tauriCdpVerifier, "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "routing workspace Tauri CDP verifier");
assertIncludes(tauriCdpVerifier, "[980, 640]", "routing workspace Tauri CDP verifier");
assertIncludes(tauriCdpVerifier, "routing-workspace-${width}x${height}.png", "routing workspace Tauri CDP verifier");
assertIncludes(tauriCdpVerifier, "request-log-opened-1024x768.png", "routing workspace Tauri CDP verifier");
assertExcludes(tauriCdpVerifier, "Remove-Item", "routing workspace Tauri CDP verifier");
assertExcludes(tauriCdpVerifier, "git add", "routing workspace Tauri CDP verifier");
assertExcludes(tauriCdpVerifier, "https://", "routing workspace Tauri CDP verifier");

console.log("routing status integration contract ok");
