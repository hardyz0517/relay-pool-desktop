import {
  getRoutingProtectionStatus,
  listErrorRateHistory,
  getRequestDecisionTrace,
  getStationKeyOperationalDetail,
  listModelAliases,
  listRecentRouteDecisions,
  loadRoutingRuntimeOverlay,
  loadRoutingPolicy,
  loadRoutingWorkspaceSnapshot,
  simulateRoute,
} from "@/lib/api/routing";
import { getSettings } from "@/lib/api/settings";
import type {
  ModelAlias,
  RecentRouteDecisionsInput,
  RecentRouteDecisionsPage,
  RequestDecisionTrace,
  ErrorRateHistoryInput,
  ErrorRateHistoryPage,
  RouteSimulationInput,
  RouteSimulationResult,
  RoutingRuntimeOverlay,
  RoutingWorkspaceSnapshot,
  RoutingWorkspaceSnapshotInput,
  RoutingProtectionStatusInput,
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
  all: ["routing"] as const,
  policy: () => ["routing", "policy"] as const,
  protectionStatus: (input: RoutingProtectionStatusInput = {}) =>
    ["routing", "protectionStatus", input.model ?? null] as const,
  errorRateHistory: (input: ErrorRateHistoryInput = {}) =>
    ["routing", "errorRateHistory", input.beforeMs ?? null, input.limit ?? null] as const,
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

export function errorRateHistoryQueryOptions(input: ErrorRateHistoryInput = {}) {
  return {
    queryKey: routingQueryKeys.errorRateHistory(input),
    queryFn: () => listErrorRateHistory(input),
    staleTime: 5_000,
    retry: false,
    meta: { suppressGlobalErrorNotification: true },
  } as const;
}

export function listErrorRateHistoryQuery(input: ErrorRateHistoryInput = {}): Promise<ErrorRateHistoryPage> {
  return listErrorRateHistory(input);
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
