import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  deleteModelAlias,
  getStationKeyCapabilities,
  getRoutingProtectionStatus,
  getRoutingPolicyPublicationStatus,
  listModelAliases,
  simulateRoute,
  updateStationKeyCapabilities,
  upsertModelAlias,
} from "./routing";

describe("routing backend cutover", () => {
  const routing = {
    getStationKeyCapabilities: vi.fn(async (stationKeyId: string) => ({
      stationKeyId,
      supportsChatCompletions: true,
      supportsResponses: true,
      supportsEmbeddings: false,
      supportsStream: true,
      supportsTools: false,
      supportsVision: false,
      supportsReasoning: false,
      modelAllowlist: [],
      modelBlocklist: [],
      preferredModels: [],
      onlyUseAsBackup: false,
      routingTags: [],
      updatedAt: "now",
    })),
    updateStationKeyCapabilities: vi.fn(async (input) => ({ ...input, updatedAt: "now" })),
    listModelAliases: vi.fn(async () => []),
    upsertModelAlias: vi.fn(async (input) => ({ id: "alias-1", createdAt: "now", updatedAt: "now", ...input })),
    deleteModelAlias: vi.fn(async () => undefined),
    getRoutingProtectionStatus: vi.fn(async () => ({
      statusVersion: "routing_protection_status_v1",
      generatedAtMs: 1,
      entries: [],
      readModelStatus: "available" as const,
      timeouts: null,
    })),
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
      aggregates: {
        totalCandidates: 0,
        schedulableCandidates: 0,
        eligibleCandidates: 0,
        conditionallyEligibleCandidates: 0,
        excludedCandidates: 0,
        unavailableCandidates: 0,
        closedCircuits: 0,
        openCircuits: 0,
        halfOpenCircuits: 0,
        persistenceUnavailableCircuits: 0,
      },
      circuitReadModelStatus: "available" as const,
      circuitReadModelCode: null,
      circuitRevision: { processGateRevision: 0, persistenceHealthRevision: 0, stateFingerprint: "test" },
      readModelStatus: "available" as const,
      plannerEvaluation: "available" as const,
      plannerEvaluationCode: null,
      availabilityStatus: "all_keys_unavailable" as const,
    })),
    loadRoutingRuntimeOverlay: vi.fn(async () => ({
      overlayVersion: "routing_runtime_overlay_v3",
      sampledAtMs: 1,
      revision: 1,
      candidates: [],
    })),
    listRecentRouteDecisions: vi.fn(async () => ({
      pageVersion: "recent_route_decisions_v1",
      decisions: [],
      nextCursor: null,
      readModelStatus: "available" as const,
    })),
    getRequestDecisionTrace: vi.fn(async (requestLogId: string) => ({
      traceVersion: "request_decision_trace_v1",
      requestLogId,
      status: "trace_unavailable" as const,
      detailAvailability: "unavailable" as const,
      reason: "trace_unavailable",
      explanationKey: "trace_unavailable",
      policyRevision: null,
      legacySummary: null,
      timeline: [],
      planningRounds: [],
    })),
    loadRoutingPolicy: vi.fn(async () => ({
      config: {
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
          optimisticLatencyMs: 2500,
        },
        retry: { version: 1, maxRetryCount: 3, consecutiveFailureThreshold: 3 },
        circuitBreaker: { version: 1, recoverySuccessThreshold: 2, recoveryWaitSeconds: 30 },
        timeoutPolicy: { version: 2, connectSeconds: 10, firstByteSeconds: 30, precommitSeconds: 60, bufferedExecutionSeconds: 300, streamIdleSeconds: 90 },
      },
      revision: 1, policyVersion: "routing-policy-v1", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 0, documentSync: null,
    })),
    getRoutingPolicyPublicationStatus: vi.fn(async (input) => ({
      revision: input.revision,
      policyGenerationId: input.policyGenerationId ?? "pg1_fixture",
      status: "staged" as const,
      failureCode: null,
      updatedAtMs: 1,
      terminal: false,
    })),
    applyRoutingPolicyDocument: vi.fn(),
    simulateRoute: vi.fn(async (input) => ({
      previewPolicyVersion: "intelligent_planner_v1",
      capacityMode: "snapshot_only",
      selectedCapacityAcquired: false,
      selectedStationKeyId: null,
      selectedStationId: null,
      mappedModel: input.model,
      policy: input.policy ?? "cost_stable_first",
      maxRateMultiplier: input.maxRateMultiplier ?? null,
      routingGroupFilter: input.routingGroupFilter ?? "all_groups",
      plannerErrorCode: null,
      candidates: [],
      message: "ok",
    })),
    getModelMappingWorkspace: vi.fn(),
    getModelMappingDocument: vi.fn(),
    validateModelMappingDocument: vi.fn(),
    applyModelMappingDocument: vi.fn(),
    restoreModelMappingRevision: vi.fn(),
    simulateModelMapping: vi.fn(),
    resolveRequestMappingTrace: vi.fn(),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ routing: routing as BackendClient["routing"] }));
    for (const fn of Object.values(routing)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes reads and mutations through the active backend client", async () => {
    const capabilities = {
      stationKeyId: "key-1",
      supportsChatCompletions: true,
      supportsResponses: true,
      supportsEmbeddings: false,
      supportsStream: true,
      supportsTools: false,
      supportsVision: false,
      supportsReasoning: false,
      modelAllowlist: ["fixture-model"],
      modelBlocklist: [],
      preferredModels: ["fixture-model"],
      onlyUseAsBackup: false,
      routingTags: ["fixture"],
    };
    const alias = {
      id: null,
      clientModel: "client-model",
      upstreamModel: "upstream-model",
      enabled: true,
      note: null,
    };
    const routeInput = {
      endpoint: "chat_completions",
      model: "client-model",
      stream: false,
      usesTools: false,
      usesVision: false,
      usesReasoning: false,
      policy: {
        version: 1,
        reliabilityWeight: 4000,
        responsivenessWeight: 2500,
        costWeight: 2000,
        preferenceWeight: 1500,
        maxCandidates: 64,
        explorationShareBasisPoints: 500,
        allowDepletedFallback: false,
         affinityEnabled: false,
         affinityTtlSeconds: 300,
         outboundProxyMode: "inherit",
         outboundProxyUrl: null,
      },
    } as const;

    await getStationKeyCapabilities("key-1");
    await updateStationKeyCapabilities(capabilities);
    await listModelAliases();
    await upsertModelAlias(alias);
    await deleteModelAlias("alias-1");
    await getRoutingProtectionStatus();
    await getRoutingPolicyPublicationStatus({ revision: 7, policyGenerationId: "pg1_fixture" });
    await simulateRoute(routeInput);

    expect(routing.getStationKeyCapabilities).toHaveBeenCalledWith("key-1");
    expect(routing.updateStationKeyCapabilities).toHaveBeenCalledWith(capabilities);
    expect(routing.listModelAliases).toHaveBeenCalledTimes(1);
    expect(routing.upsertModelAlias).toHaveBeenCalledWith(alias);
    expect(routing.deleteModelAlias).toHaveBeenCalledWith("alias-1");
    expect(routing.getRoutingProtectionStatus).toHaveBeenCalledWith();
    expect(routing.getRoutingPolicyPublicationStatus).toHaveBeenCalledWith({
      revision: 7,
      policyGenerationId: "pg1_fixture",
    });
    expect(routing.simulateRoute).toHaveBeenCalledWith(routeInput);
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
