import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const page = read("src/features/routing/RoutingPage.tsx");
const diagnostics = read("src/features/routing/RoutingStatusDiagnosticsPanel.tsx");
const routingApi = read("src/lib/api/routing.ts");
const routingQueries = read("src/lib/queries/routingQueries.ts");
const generated = read("src/lib/bridge/generated.ts");
const registry = read("src-tauri/src/ipc/registry.rs");

for (const forbidden of [
  "RoutingMigrationReadinessPanel",
  "routingMigrationReadiness",
  "confirmHierarchicalRoutingMigration",
  "RoutingOperationalPreviewPanel",
  'value: "workspace"',
  'label: "工作台"',
  'value: "migration"',
  'label: "迁移"',
]) {
  assert.doesNotMatch(
    page,
    new RegExp(escapeRegExp(forbidden), "u"),
    `RoutingPage must not expose legacy migration or standalone workspace UI: ${forbidden}`,
  );
}

for (const required of [
  "RoutingStatusDiagnosticsPanel",
  "loadRoutingWorkspaceSnapshotQuery",
  "loadRoutingRuntimeOverlayQuery",
  "listRecentRouteDecisionsQuery",
  'activeTab === "status"',
]) {
  assert.match(
    page,
    new RegExp(escapeRegExp(required), "u"),
    `RoutingPage must fold operational routing data into the status tab through ${required}`,
  );
}

for (const required of [
  "simulateRouteQuery",
  "snapshot.productionPolicy",
  "snapshot.maxRateMultiplier",
  "snapshot.routingGroupFilter",
  "decisions?.decisions",
  "runtimeOverlay?.candidates",
]) {
  assert.match(
    diagnostics,
    new RegExp(escapeRegExp(required), "u"),
    `routing status diagnostics must reuse backend operational read-model data: ${required}`,
  );
}

for (const forbidden of [
  "previewPolicyVersion",
  "capacityMode",
  "selectedCapacityAcquired",
  "runtime rev",
  "deep link",
  "Operational detail",
]) {
  assert.doesNotMatch(
    diagnostics,
    new RegExp(escapeRegExp(forbidden), "u"),
    `routing status diagnostics must not expose internal workspace jargon: ${forbidden}`,
  );
}

for (const required of [
  "loadRoutingWorkspaceSnapshot",
  "loadRoutingRuntimeOverlay",
  "listRecentRouteDecisions",
  "getStationKeyOperationalDetail",
  "getRequestDecisionTrace",
  "simulateRoute",
]) {
  assert.match(
    routingApi,
    new RegExp(`export function ${required}\\b`, "u"),
    `routing API must expose ${required}`,
  );
  assert.match(
    routingQueries,
    new RegExp(`${required}(?:Query)?`, "u"),
    `routing query layer must consume ${required}`,
  );
}

for (const command of [
  "load_routing_workspace_snapshot",
  "load_routing_runtime_overlay",
  "list_recent_route_decisions",
  "get_station_key_operational_detail",
  "get_request_decision_trace",
  "simulate_route",
]) {
  assert.match(
    registry,
    new RegExp(escapeRegExp(command), "u"),
    `IPC registry must register ${command}`,
  );
}

for (const binding of [
  "loadRoutingWorkspaceSnapshot",
  "loadRoutingRuntimeOverlay",
  "listRecentRouteDecisions",
  "getStationKeyOperationalDetail",
  "getRequestDecisionTrace",
  "simulateRoute",
]) {
  assert.match(
    generated,
    new RegExp(`export function ${binding}\\b`, "u"),
    `generated bridge must expose ${binding}`,
  );
}

console.log("routing operational status architecture checks passed");

function read(path) {
  return readFileSync(path, "utf8");
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}
