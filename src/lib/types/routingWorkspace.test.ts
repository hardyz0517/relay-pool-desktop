import { describe, expect, it } from "vitest";
import type { ProxyStatus } from "./proxy";
import type {
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
} from "./routing";
import { toRoutingWorkspaceView } from "./routingWorkspace";

describe("routing workspace view", () => {
  it("keeps request eligibility separate from the administrative schedulable switch", () => {
    const excluded = candidate({
      schedulable: true,
      hardRejectionCodes: ["group_mismatch"],
    });
    const paused = candidate({
      stationKeyId: "paused",
      schedulable: false,
      hardRejectionCodes: ["candidate_unschedulable"],
    });

    const view = toRoutingWorkspaceView(snapshot([excluded, paused]), proxyStatus());

    expect(view.candidates[0]).toMatchObject({
      enabled: true,
      schedulable: true,
      previewEligible: false,
      routingGroupMatch: false,
    });
    expect(view.candidates[1]).toMatchObject({
      enabled: true,
      schedulable: false,
      previewEligible: false,
      routingGroupMatch: true,
    });
  });

  it("merges current concurrency only from the matching runtime candidate revision", () => {
    const current = toRoutingWorkspaceView(
      snapshot([candidate()]),
      proxyStatus(),
      runtimeOverlay({ stationKeyInFlight: 3 }),
    );
    const stale = toRoutingWorkspaceView(
      snapshot([candidate()]),
      proxyStatus(),
      runtimeOverlay({ endpointRevision: 0, stationKeyInFlight: 7 }),
    );

    expect(current.candidates[0].currentConcurrency).toBe(3);
    expect(stale.candidates[0].currentConcurrency).toBeNull();
  });
});

function runtimeOverlay(
  overrides: Partial<RoutingRuntimeOverlay["candidates"][number]> = {},
): RoutingRuntimeOverlay {
  return {
    overlayVersion: "routing_runtime_overlay_v2",
    sampledAtMs: 2,
    revision: 1,
    candidates: [
      {
        stationKeyId: "key-1",
        stationId: "station-1",
        endpointRevision: 1,
        inFlight: 0,
        stationKeyInFlight: 0,
        healthState: "ready",
        cooldownUntil: null,
        ...overrides,
      },
    ],
  };
}

function candidate(overrides: Partial<RoutingWorkspaceCandidate> = {}): RoutingWorkspaceCandidate {
  return {
    stationKeyId: "key-1",
    stationId: "station-1",
    stationName: "Station",
    keyName: "Key",
    endpointRevision: 1,
    priority: 0,
    schedulable: true,
    healthState: "ready",
    score: null,
    scoreDetails: null,
    group: null,
    multiplier: {
      status: "resolved",
      multiplier: 0.1,
      selectedSource: "collector",
      ceilingRejected: false,
      reason: "canonical_economic_snapshot",
    },
    capabilitySummary: {
      chatCompletions: true,
      responses: true,
      embeddings: false,
      stream: true,
      tools: false,
      vision: false,
      reasoning: false,
    },
    capabilityVerdicts: {
      protocol: "allow",
      model: "allow",
      stream: "allow",
      tools: "deny",
      vision: "deny",
      reasoning: "deny",
      rejectionSubjects: [],
    },
    priceBasis: "multiplier_proxy",
    pricing: {
      basis: "multiplier_proxy",
      comparisonValue: 0.1,
      reason: null,
      currency: null,
      unit: "rate_multiplier",
      estimatedInputPrice: null,
      estimatedOutputPrice: null,
      statusLabel: "multiplier_proxy",
      sourceChain: [],
      observedAt: null,
      confidence: 0.9,
    },
    balanceStatus: "normal",
    balanceValue: 10,
    balanceCurrency: "USD",
    capacity: { mode: "snapshot_only", maxConcurrency: 4, inFlight: 0, acquired: false },
    failureDomain: {
      kind: "capacity_domain",
      resolution: "resolved",
      providerFamily: "OpenAI",
      deploymentIdentity: "primary",
      regionIdentity: "US",
      revision: 1,
      commitment: "v1:fixture-domain",
      explanationKey: "routing.failure_domain.resolved",
    },
    sourceRefs: {
      stationKeyId: "key-1",
      stationId: "station-1",
      endpointRevision: 1,
      snapshotId: "snapshot-1",
      factVersionVector: "endpoint:1",
      projectorVersion: "routing_workspace_canonical_v1",
    },
    hardRejectionCodes: [],
    ...overrides,
  };
}

function snapshot(candidates: RoutingWorkspaceCandidate[]): RoutingWorkspaceSnapshot {
  return {
    readModelVersion: "routing_workspace_read_model_v1",
    generatedAtMs: 1,
    policyConfig: {
      version: 2,
      reliabilityWeight: 4000,
      responsivenessWeight: 2500,
      costWeight: 2000,
      preferenceWeight: 1500,
      maxCandidates: 64,
      explorationShareBasisPoints: 500,
       allowDepletedFallback: false,
       affinityEnabled: false,
       affinityTtlSeconds: 300,
       maxRateMultiplier: null,
       routingGroupFilter: "all_groups",
       outboundProxyMode: "inherit",
       outboundProxyUrl: null,
       retryFailover: { version: 2, maxTotalAttempts: 4, maxSameTargetCapacityRetries: 2, capacityRetryWaitBudgetSeconds: 2, allowCrossCapacityDomainFallback: true },
       protectionProfile: { version: 2, enabled: false, windowMaxSamples: 64, windowSeconds: 300, minSamples: 5, failureThresholdPercent: 60, halfOpenSuccessesToClose: 2 },
       timeoutPolicy: { version: 2, connectSeconds: 10, firstByteSeconds: 30, precommitSeconds: 60, bufferedExecutionSeconds: 300, streamIdleSeconds: 90 },
    },
    previewPolicyVersion: "intelligent_planner_v1",
    maxRateMultiplier: null,
    routingGroupFilter: "all_groups",
    capacityMode: "snapshot_only",
    page: { limit: 128, returned: candidates.length, nextCursor: null },
    candidates,
    readModelStatus: "available",
  };
}

function proxyStatus(): ProxyStatus {
  return {
    running: false,
    lifecycle: "stopped",
    bindAddr: "127.0.0.1",
    port: 8787,
    startedAt: null,
    lastError: null,
    activeRequests: 0,
    requestCount: 0,
  };
}
