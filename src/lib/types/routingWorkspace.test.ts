import { describe, expect, it } from "vitest";
import type { ProxyStatus } from "./proxy";
import type {
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
} from "./routing";
import { toRoutingWorkspaceView } from "./routingWorkspace";

describe("routing workspace view", () => {
  it("preserves v3 circuit diagnostics for candidate status rendering", () => {
    const diagnostics = {
      circuit: {
        state: "half_open" as const,
        stateRevision: 3,
        lifecycleRevision: 2,
        policyRevision: 1,
        persistenceStatus: "available" as const,
        stateRowPresent: true,
        consecutiveFailures: 4,
        reopenLevel: 1,
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        halfOpenLeaseInFlight: true,
        halfOpenLeaseExpiresAtMs: 123,
        recoverySuccesses: 1,
        scoreGateStatus: "passed" as const,
        scoreGateReason: "half_open_lease_in_flight",
        bestClosedEffectiveScore: 8800,
      },
      effectiveScore: 8800,
      baseScore: 8800,
      quality: null,
      attempts: {
        rawRealAttemptCount: 1,
        deduplicatedRealRequestCount: 1,
      },
    } as NonNullable<RoutingWorkspaceCandidate["diagnostics"]>;

    const view = toRoutingWorkspaceView(
      snapshot([candidate({ diagnostics })]),
      proxyStatus(),
    );

    expect(view.candidates[0].diagnostics?.circuit).toEqual(diagnostics.circuit);
  });

  it("keeps request eligibility separate from the administrative schedulable switch", () => {
    const excluded = candidate({
      schedulable: true,
      participationStatus: "excluded",
      participationReason: "planner_excluded",
      scoreStatus: "excluded",
      plannerExclusionCodes: ["group_mismatch"],
      hardRejectionCodes: ["group_mismatch"],
    });
    const paused = candidate({
      stationKeyId: "paused",
      schedulable: false,
      participationStatus: "excluded",
      participationReason: "administratively_disabled",
      scoreStatus: "excluded",
      plannerExclusionCodes: ["candidate_unschedulable"],
      hardRejectionCodes: ["candidate_unschedulable"],
    });

    const view = toRoutingWorkspaceView(snapshot([excluded, paused]), proxyStatus());

    expect(view.candidates[0]).toMatchObject({
      enabled: true,
      schedulable: true,
      participationStatus: "excluded",
      participationReason: "planner_excluded",
      routingGroupMatch: false,
    });
    expect(view.candidates[1]).toMatchObject({
      enabled: true,
      schedulable: false,
      participationStatus: "excluded",
      participationReason: "administratively_disabled",
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

  it("does not resurrect stale canonical balance rejection for an assessed score", () => {
    const view = toRoutingWorkspaceView(
      snapshot([
        candidate({
          score: 7100,
          scoreDetails: null,
          scoreStatus: "scored",
          plannerExclusionCodes: [],
          balanceStatus: "depleted",
          balanceValue: 3.61,
          hardRejectionCodes: ["balance_depleted"],
        }),
      ]),
      proxyStatus(),
    );

    expect(view.candidates[0]).toMatchObject({
      scoreStatus: "scored",
      participationStatus: "eligible",
      participationReason: "ready",
      plannerExclusionCodes: [],
      balanceValue: 3.61,
    });
  });

  it("uses full-set backend aggregates instead of recounting the current page", () => {
    const paged = snapshot([candidate()]);
    paged.aggregates = {
      totalCandidates: 5,
      schedulableCandidates: 4,
      eligibleCandidates: 1,
      conditionallyEligibleCandidates: 1,
      excludedCandidates: 3,
      unavailableCandidates: 0,
      closedCircuits: 3,
      openCircuits: 1,
      halfOpenCircuits: 1,
      persistenceUnavailableCircuits: 0,
    };

    const view = toRoutingWorkspaceView(paged, proxyStatus());

    expect(view.summary).toMatchObject({
      totalCandidateCount: 5,
      currentPageCandidateCount: 1,
      participatingCandidateCount: 2,
      nonParticipatingCandidateCount: 3,
    });
  });
});

function runtimeOverlay(
  overrides: Partial<RoutingRuntimeOverlay["candidates"][number]> = {},
): RoutingRuntimeOverlay {
  return {
    overlayVersion: "routing_runtime_overlay_v3",
    sampledAtMs: 2,
    revision: 1,
    candidates: [
      {
        stationKeyId: "key-1",
        stationId: "station-1",
        endpointRevision: 1,
        inFlight: 0,
        stationKeyInFlight: 0,
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
    participationStatus: "eligible",
    participationReason: "ready",
    score: null,
    scoreStatus: "unavailable",
    plannerExclusionCodes: [],
    assessmentSnapshotId: null,
    assessmentDurableRevision: null,
    assessmentRequestContextFingerprint: null,
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
    capacity: { mode: "snapshot_only", status: "available", maxConcurrency: 4, inFlight: 0, acquired: false },
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
      routingGroupFilter: "all_groups",
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
    routingGroupFilter: "all_groups",
    capacityMode: "snapshot_only",
    page: { limit: 128, returned: candidates.length, nextCursor: null },
    candidates,
    aggregates: {
      totalCandidates: candidates.length,
      schedulableCandidates: candidates.filter((candidate) => candidate.schedulable).length,
      eligibleCandidates: candidates.filter((candidate) => candidate.participationStatus === "eligible").length,
      conditionallyEligibleCandidates: candidates.filter((candidate) => candidate.participationStatus === "conditionally_eligible").length,
      excludedCandidates: candidates.filter((candidate) => candidate.participationStatus === "excluded").length,
      unavailableCandidates: candidates.filter((candidate) => candidate.participationStatus === "unavailable").length,
      closedCircuits: candidates.length,
      openCircuits: 0,
      halfOpenCircuits: 0,
      persistenceUnavailableCircuits: 0,
    },
    circuitReadModelStatus: "available",
    circuitReadModelCode: null,
    circuitRevision: {
      processGateRevision: 0,
      persistenceHealthRevision: 0,
      stateFingerprint: "test",
    },
    readModelStatus: "available",
    plannerEvaluation: "available",
    plannerEvaluationCode: null,
    availabilityStatus: "available",
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
