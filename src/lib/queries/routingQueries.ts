import {
  getRequestDecisionTrace,
  getStationKeyOperationalDetail,
  listModelAliases,
  listRecentRouteDecisions,
  loadRoutingRuntimeOverlay,
  loadRoutingWorkspaceSnapshot,
  simulateRoute,
} from "@/lib/api/routing";
import { getSettings } from "@/lib/api/settings";
import type {
  ModelAlias,
  RecentRouteDecisionsInput,
  RecentRouteDecisionsPage,
  RequestDecisionTrace,
  RouteSimulationInput,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceSnapshot,
  RoutingWorkspaceSnapshotInput,
  StationKeyOperationalDetail,
} from "@/lib/types/routing";
import type { AppSettings } from "@/lib/types/settings";

export type RoutingWorkspace = {
  settings: AppSettings;
  modelAliases: ModelAlias[];
};

export async function loadRoutingWorkspace(): Promise<RoutingWorkspace> {
  const [settings, modelAliases] = await Promise.all([getSettings(), listModelAliases()]);

  return {
    settings,
    modelAliases,
  };
}

export const routingQueryKeys = {
  workspaceSnapshot: (input: RoutingWorkspaceSnapshotInput = {}) =>
    ["routing", "workspaceSnapshot", input.limit ?? null, input.cursor ?? null] as const,
  runtimeOverlay: () => ["routing", "runtimeOverlay"] as const,
  recentDecisions: (input: RecentRouteDecisionsInput = {}) =>
    ["routing", "recentDecisions", input.limit ?? null, input.cursor ?? null] as const,
  operationalDetail: (stationKeyId: string) =>
    ["routing", "operationalDetail", stationKeyId] as const,
  decisionTrace: (requestLogId: string) => ["routing", "decisionTrace", requestLogId] as const,
  simulation: (input: RouteSimulationInput) =>
    [
      "routing",
      "simulation",
      input.endpoint,
      input.model,
      input.stream,
      input.usesTools,
      input.usesVision,
      input.usesReasoning,
      input.policy,
      input.maxRateMultiplier ?? null,
      input.routingGroupFilter ?? null,
      input.sessionHash ?? null,
      input.previousResponseId ?? null,
    ] as const,
};

export function loadRoutingWorkspaceSnapshotQuery(
  input: RoutingWorkspaceSnapshotInput = {},
): Promise<RoutingWorkspaceSnapshot> {
  return loadRoutingWorkspaceSnapshot(input);
}

export function loadRoutingRuntimeOverlayQuery(): Promise<RoutingRuntimeOverlay> {
  return loadRoutingRuntimeOverlay();
}

export function listRecentRouteDecisionsQuery(
  input: RecentRouteDecisionsInput = {},
): Promise<RecentRouteDecisionsPage> {
  return listRecentRouteDecisions(input);
}

export function getStationKeyOperationalDetailQuery(
  stationKeyId: string,
): Promise<StationKeyOperationalDetail> {
  return getStationKeyOperationalDetail(stationKeyId);
}

export function getRequestDecisionTraceQuery(requestLogId: string): Promise<RequestDecisionTrace> {
  return getRequestDecisionTrace(requestLogId);
}

export function simulateRouteQuery(input: RouteSimulationInput): Promise<RouteSimulationResult> {
  return simulateRoute(input);
}
