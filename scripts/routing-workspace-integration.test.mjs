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
const shellRegistry = read("src/app/shellPageRegistry.tsx");
const keyPoolPage = read("src/features/key-pool/KeyPoolPage.tsx");
const logsPage = read("src/features/logs/LogsPage.tsx");

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
assertIncludes(workspacePanel, "Downstream / cost aggregate", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "deepLink.kind === \"station-key\"", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "deepLink.kind === \"request\"", "RoutingOperationalPreviewPanel");
assertIncludes(workspacePanel, "deepLink.kind === \"simulate-model\"", "RoutingOperationalPreviewPanel");
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

assertIncludes(shellRegistry, "openRoutingDeepLink", "shellPageRegistry");
assertIncludes(shellRegistry, "routingDeepLink", "shellPageRegistry");
assertIncludes(keyPoolPage, 'kind: "station-key"', "KeyPoolPage");
assertIncludes(logsPage, 'kind: "request"', "LogsPage");
assertIncludes(logsPage, "查看路由链路", "LogsPage");

console.log("routing workspace integration contract ok");
