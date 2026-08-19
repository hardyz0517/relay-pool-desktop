import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  RecentRouteDecisionsInput,
  RouteSimulationInput,
  RoutingWorkspaceSnapshotInput,
  UpdateStationKeyCapabilitiesInput,
  UpsertModelAliasInput,
} from "@/lib/types/routing";

export function getStationKeyCapabilities(stationKeyId: string) {
  return getActiveBackendClient().routing.getStationKeyCapabilities(stationKeyId);
}

export function updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInput) {
  return getActiveBackendClient().routing.updateStationKeyCapabilities(input);
}

export function listModelAliases() {
  return getActiveBackendClient().routing.listModelAliases();
}

export function upsertModelAlias(input: UpsertModelAliasInput) {
  return getActiveBackendClient().routing.upsertModelAlias(input);
}

export function deleteModelAlias(id: string) {
  return getActiveBackendClient().routing.deleteModelAlias(id);
}

export function listStationKeyHealth() {
  return getActiveBackendClient().routing.listStationKeyHealth();
}

export function loadRoutingPolicy() {
  return getActiveBackendClient().routing.loadRoutingPolicy();
}

export function updateRoutingPolicy(input: import("@/lib/types/routing").UpdateRoutingPolicyInput) {
  return getActiveBackendClient().routing.updateRoutingPolicy(input);
}

export function applyRoutingPolicyDocument(
  input: import("@/lib/types/routing").ApplyRoutingPolicyDocumentInput,
) {
  return getActiveBackendClient().routing.applyRoutingPolicyDocument(input);
}

export function loadRoutingWorkspaceSnapshot(input: RoutingWorkspaceSnapshotInput = {}) {
  return getActiveBackendClient().routing.loadRoutingWorkspaceSnapshot(input);
}

export function loadRoutingRuntimeOverlay() {
  return getActiveBackendClient().routing.loadRoutingRuntimeOverlay();
}

export function listRecentRouteDecisions(input: RecentRouteDecisionsInput = {}) {
  return getActiveBackendClient().routing.listRecentRouteDecisions(input);
}

export function getStationKeyOperationalDetail(stationKeyId: string) {
  return getActiveBackendClient().routing.getStationKeyOperationalDetail(stationKeyId);
}

export function getRequestDecisionTrace(requestLogId: string) {
  return getActiveBackendClient().routing.getRequestDecisionTrace(requestLogId);
}

export function getStationKeyHealth(stationKeyId: string) {
  return getActiveBackendClient().routing.getStationKeyHealth(stationKeyId);
}

export function simulateRoute(input: RouteSimulationInput) {
  return getActiveBackendClient().routing.simulateRoute(input);
}
