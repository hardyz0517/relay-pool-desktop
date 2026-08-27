import {
  getRoutingProtectionStatus,
  getRequestDecisionTrace,
  getStationKeyOperationalDetail,
  listRecentRouteDecisions,
  loadRoutingRuntimeOverlay,
  loadRoutingPolicy,
  loadRoutingWorkspaceSnapshot,
  simulateRoute,
} from "@/lib/api/routing";
import type {
  RecentRouteDecisionsInput,
  RecentRouteDecisionsPage,
  RequestDecisionTrace,
  RouteSimulationInput,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceSnapshot,
  RoutingWorkspaceSnapshotInput,
  RoutingProtectionStatusInput,
  StationKeyOperationalDetail,
} from "@/lib/types/routing";

export const routingQueryKeys = {
  all: ["routing"] as const,
  policy: () => ["routing", "policy"] as const,
  protectionStatus: (input: RoutingProtectionStatusInput = {}) =>
    ["routing", "protectionStatus", input.model ?? null] as const,
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

/**
 * The policy document is an authoritative, revisioned read model. Keep this
 * query separate from workspace projections: editing it must never turn a
 * runtime overlay refresh into an implicit draft write.
 */
export function routingPolicyQueryOptions() {
  return {
    queryKey: routingQueryKeys.policy(),
    queryFn: loadRoutingPolicy,
    staleTime: 0,
    retry: false,
  } as const;
}

export function routingProtectionStatusQueryOptions(input: RoutingProtectionStatusInput = {}) {
  return {
    queryKey: routingQueryKeys.protectionStatus(input),
    queryFn: () => getRoutingProtectionStatus(input),
    staleTime: 5_000,
    retry: false,
    meta: { suppressGlobalErrorNotification: true },
  } as const;
}

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
