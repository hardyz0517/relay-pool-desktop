import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  deleteModelAlias,
  getStationKeyCapabilities,
  getStationKeyHealth,
  listModelAliases,
  listStationKeyHealth,
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
    listStationKeyHealth: vi.fn(async () => []),
    loadRoutingWorkspaceSnapshot: vi.fn(async () => ({
      readModelVersion: "routing_workspace_read_model_v1",
      generatedAtMs: 1,
      policyConfig: {
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
      },
      previewPolicyVersion: "intelligent_planner_v1",
      maxRateMultiplier: null,
      routingGroupFilter: "all_groups" as const,
      capacityMode: "snapshot_only" as const,
      page: { limit: 50, returned: 0, nextCursor: null },
      candidates: [],
      readModelStatus: "available" as const,
    })),
    loadRoutingRuntimeOverlay: vi.fn(async () => ({
      overlayVersion: "routing_runtime_overlay_v1",
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
    getStationKeyOperationalDetail: vi.fn(async (stationKeyId: string) => ({
      detailVersion: "station_key_operational_detail_v1",
      stationKeyId,
      stationId: "station-1",
      endpointRevision: 1,
      facts: [],
      lazyHistoryAvailable: true,
      readModelStatus: "available" as const,
    })),
    getRequestDecisionTrace: vi.fn(async (requestLogId: string) => ({
      traceVersion: "request_decision_trace_v1",
      requestLogId,
      status: "trace_unavailable" as const,
      reason: "trace_unavailable",
      legacySummary: null,
      timeline: [],
      planningRounds: [],
    })),
    getStationKeyHealth: vi.fn(async (stationKeyId: string) => ({
      stationKeyId,
      lastSuccessAt: null,
      lastFailureAt: null,
      consecutiveFailures: 0,
      successCount: 0,
      failureCount: 0,
      avgLatencyMs: null,
      lastErrorSummary: null,
      cooldownUntil: null,
      updatedAt: "now",
    })),
    loadRoutingPolicy: vi.fn(async () => ({
      config: { version: 1, reliabilityWeight: 4000, responsivenessWeight: 2500, costWeight: 2000, preferenceWeight: 1500, maxCandidates: 64, explorationShareBasisPoints: 500, allowDepletedFallback: false, affinityEnabled: false, affinityTtlSeconds: 300 },
      revision: 1, policyVersion: "routing-policy-v1", systemVersion: "routing-system-v1", status: "active", updatedAtMs: 0,
    })),
    updateRoutingPolicy: vi.fn(),
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
      },
    } as const;

    await getStationKeyCapabilities("key-1");
    await updateStationKeyCapabilities(capabilities);
    await listModelAliases();
    await upsertModelAlias(alias);
    await deleteModelAlias("alias-1");
    await listStationKeyHealth();
    await getStationKeyHealth("key-1");
    await simulateRoute(routeInput);

    expect(routing.getStationKeyCapabilities).toHaveBeenCalledWith("key-1");
    expect(routing.updateStationKeyCapabilities).toHaveBeenCalledWith(capabilities);
    expect(routing.listModelAliases).toHaveBeenCalledTimes(1);
    expect(routing.upsertModelAlias).toHaveBeenCalledWith(alias);
    expect(routing.deleteModelAlias).toHaveBeenCalledWith("alias-1");
    expect(routing.listStationKeyHealth).toHaveBeenCalledTimes(1);
    expect(routing.getStationKeyHealth).toHaveBeenCalledWith("key-1");
    expect(routing.simulateRoute).toHaveBeenCalledWith(routeInput);
  });
});

function testBackendClient(overrides: Partial<BackendClient>): BackendClient {
  return {
    mode: "desktop",
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    changeEvents: {} as BackendClient["changeEvents"],
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
