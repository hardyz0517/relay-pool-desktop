import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  getRequestDecisionTraceQuery,
  getStationKeyOperationalDetailQuery,
  listRecentRouteDecisionsQuery,
  loadRoutingRuntimeOverlayQuery,
  loadRoutingWorkspaceSnapshotQuery,
  routingPolicyQueryOptions,
  routingPolicyPublicationQueryOptions,
  routingProtectionStatusQueryOptions,
  routingQueryKeys,
  simulateRouteQuery,
} from "./routingQueries";

describe("routing query owner", () => {
  const routing = {
    getStationKeyCapabilities: vi.fn(),
    updateStationKeyCapabilities: vi.fn(),
    listModelAliases: vi.fn(),
    upsertModelAlias: vi.fn(),
    deleteModelAlias: vi.fn(),
    listStationKeyHealth: vi.fn(),
    getRoutingProtectionStatus: vi.fn(),
    loadRoutingWorkspaceSnapshot: vi.fn(async () => ({
      readModelVersion: "routing_workspace_read_model_v3",
      generatedAtMs: 1,
      policyConfig: {
        version: 3,
        reliabilityWeight: 4000,
        responsivenessWeight: 2500,
        costWeight: 2000,
        preferenceWeight: 1500,
        allowDepletedFallback: false,
        affinityEnabled: false,
        affinityTtlSeconds: 300,
        maxRateMultiplier: null,
        routingGroupFilter: "all_groups" as const,
        outboundProxyMode: "inherit",
        outboundProxyUrl: null,
        reliabilitySourceWeights: { realTrafficPercent: 70, monitoringPercent: 30 },
        reliabilitySampling: {
          historicalMinimumSamples: 15,
          recentMinimumSamples: 5,
          optimisticReliabilityPercent: 95,
          optimisticLatencyMs: 2_500,
        },
        retry: { version: 1, maxRetryCount: 3, consecutiveFailureThreshold: 3 },
        circuitBreaker: { version: 1, recoverySuccessThreshold: 2, recoveryWaitSeconds: 30 },
        timeoutPolicy: { version: 2, connectSeconds: 10, firstByteSeconds: 30, precommitSeconds: 60, bufferedExecutionSeconds: 300, streamIdleSeconds: 90 },
      },
      previewPolicyVersion: "intelligent_planner_v3",
      maxRateMultiplier: null,
      routingGroupFilter: "all_groups" as const,
      capacityMode: "snapshot_only" as const,
      page: { limit: 50, returned: 0, nextCursor: null },
      candidates: [],
      readModelStatus: "available" as const,
      plannerEvaluation: "available" as const,
      plannerEvaluationCode: null,
      availabilityStatus: "all_keys_unavailable" as const,
    })),
    loadRoutingRuntimeOverlay: vi.fn(async () => ({
      overlayVersion: "routing_runtime_overlay_v2",
      sampledAtMs: 2,
      revision: 1,
      candidates: [],
    })),
    listRecentRouteDecisions: vi.fn(async () => ({
      pageVersion: "recent_route_decisions_v1",
      decisions: [],
      nextCursor: null,
      readModelStatus: "available" as const,
    })),
    getStationKeyOperationalDetail: vi.fn(async () => ({
      detailVersion: "station_key_operational_detail_v1",
      stationKeyId: "key-1",
      stationId: "station-1",
      endpointRevision: 1,
      facts: [],
      lazyHistoryAvailable: true,
      readModelStatus: "available" as const,
    })),
    getRequestDecisionTrace: vi.fn(async () => ({
      traceVersion: "request_decision_trace_v1",
      requestLogId: "request-log-1",
      status: "legacy_summary" as const,
      detailAvailability: "summary_only" as const,
      reason: "legacy_summary_only_before_cutover",
      explanationKey: "legacy_summary_only_before_cutover",
      policyRevision: null,
      legacySummary: null,
      timeline: [],
      planningRounds: [],
    })),
    getStationKeyHealth: vi.fn(),
    loadRoutingPolicy: vi.fn(),
    getRoutingPolicyPublicationStatus: vi.fn(async (input) => ({
      revision: input.revision,
      policyGenerationId: input.policyGenerationId ?? "pg1_fixture",
      status: "staged" as const,
      failureCode: null,
      updatedAtMs: 1,
      terminal: false,
    })),
    applyRoutingPolicyDocument: vi.fn(),
    simulateRoute: vi.fn(async () => ({
      previewPolicyVersion: "intelligent_planner_v1",
      capacityMode: "snapshot_only",
      selectedCapacityAcquired: false,
      selectedStationKeyId: null,
      selectedStationId: null,
      mappedModel: null,
      policy: "cost_stable_first" as const,
      maxRateMultiplier: null,
      routingGroupFilter: "all_groups" as const,
      plannerErrorCode: null,
      candidates: [],
      message: "preview",
    })),
    getModelMappingWorkspace: vi.fn(),
    getModelMappingDocument: vi.fn(),
    validateModelMappingDocument: vi.fn(),
    applyModelMappingDocument: vi.fn(),
    restoreModelMappingRevision: vi.fn(),
    simulateModelMapping: vi.fn(),
    resolveRequestMappingTrace: vi.fn(),
  } satisfies BackendClient["routing"];

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ routing }));
    for (const fn of Object.values(routing)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("keeps durable workspace and runtime overlay cache keys separate", () => {
    expect(routingQueryKeys.all).toEqual(["routing"]);
    expect(routingQueryKeys.workspaceSnapshot({ limit: 50, cursor: null })).toEqual([
      "routing",
      "workspaceSnapshot",
      50,
      null,
    ]);
    expect(routingQueryKeys.runtimeOverlay()).toEqual(["routing", "runtimeOverlay"]);
    expect(routingQueryKeys.workspaceSnapshot({ limit: 50 })).not.toEqual(
      routingQueryKeys.runtimeOverlay(),
    );
    expect(routingQueryKeys.policy()).toEqual(["routing", "policy"]);
    expect(routingQueryKeys.policyPublication({ revision: 7 })).toEqual([
      "routing",
      "policyPublication",
      7,
      null,
    ]);
    expect(routingQueryKeys.policyPublication({
      revision: 7,
      policyGenerationId: "pg1_fixture",
    })).toEqual(["routing", "policyPublication", 7, "pg1_fixture"]);
    expect(routingQueryKeys.protectionStatus()).toEqual(["routing", "protectionStatus"]);
  });

  it("keeps policy and protection reads as shared query owners", async () => {
    expect(routingPolicyQueryOptions().queryKey).toEqual(routingQueryKeys.policy());
    expect(routingProtectionStatusQueryOptions().queryKey).toEqual(routingQueryKeys.protectionStatus());
    await routingPolicyQueryOptions().queryFn();
    await routingProtectionStatusQueryOptions().queryFn();
    expect(routing.loadRoutingPolicy).toHaveBeenCalledTimes(1);
    expect(routing.getRoutingProtectionStatus).toHaveBeenCalledTimes(1);
  });

  it("queries publication status with both revision and generation fence", async () => {
    const input = { revision: 7, policyGenerationId: "pg1_fixture" };
    const options = routingPolicyPublicationQueryOptions(input);
    expect(options.queryKey).toEqual(routingQueryKeys.policyPublication(input));
    await options.queryFn();
    expect(routing.getRoutingPolicyPublicationStatus).toHaveBeenCalledWith(input);
  });

  it("keeps the protection read unscoped after capacity-domain retirement", async () => {
    await routingProtectionStatusQueryOptions().queryFn();
    expect(routing.getRoutingProtectionStatus).toHaveBeenCalledWith();
  });

  it("loads runtime overlay without refreshing workspace or history read models", async () => {
    await loadRoutingRuntimeOverlayQuery();

    expect(routing.loadRoutingRuntimeOverlay).toHaveBeenCalledTimes(1);
    expect(routing.loadRoutingWorkspaceSnapshot).not.toHaveBeenCalled();
    expect(routing.listRecentRouteDecisions).not.toHaveBeenCalled();
  });

  it("routes all routing read models through the routing backend domain", async () => {
    await loadRoutingWorkspaceSnapshotQuery({ limit: 20 });
    await listRecentRouteDecisionsQuery({ limit: 10 });
    await getStationKeyOperationalDetailQuery("key-1");
    await getRequestDecisionTraceQuery("request-log-1");
    await simulateRouteQuery({
      endpoint: "chat_completions",
      model: "gpt-5-mini",
      stream: true,
      usesTools: false,
      usesVision: false,
      usesReasoning: false,
      policy: null,
    });

    expect(routing.loadRoutingWorkspaceSnapshot).toHaveBeenCalledWith({ limit: 20 });
    expect(routing.listRecentRouteDecisions).toHaveBeenCalledWith({ limit: 10 });
    expect(routing.getStationKeyOperationalDetail).toHaveBeenCalledWith("key-1");
    expect(routing.getRequestDecisionTrace).toHaveBeenCalledWith("request-log-1");
    expect(routing.simulateRoute).toHaveBeenCalledTimes(1);
  });
});

function testBackendClient(overrides: Partial<BackendClient>): BackendClient {
  return {
    mode: "desktop",
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    alerting: {} as BackendClient["alerting"],
    collectorRuns: {} as BackendClient["collectorRuns"],
    collectors: {} as BackendClient["collectors"],
    proxy: {} as BackendClient["proxy"],
    dashboard: {} as BackendClient["dashboard"],
    runtime: {} as BackendClient["runtime"],
    dataRecovery: {} as BackendClient["dataRecovery"],
    dataMigration: {} as BackendClient["dataMigration"],
    economics: {} as BackendClient["economics"],
    groupFacts: {} as BackendClient["groupFacts"],
    pricing: {} as BackendClient["pricing"],
    routing: {} as BackendClient["routing"],
    channels: {} as BackendClient["channels"],
    updater: {} as BackendClient["updater"],
    handshake: vi.fn(async () => ({}) as never),
    ...overrides,
  };
}
