import type { ProxyStatus } from "./proxy";
import type {
  RouteEndpointKind,
  RoutingCandidateParticipationReason,
  RoutingCandidateParticipationStatus,
  RoutingGroupFilter,
  RoutingPlannerEvaluationStatus,
  RoutingScoreStatus,
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
} from "./routing";
export type DecisionFact = {
  kind: "capability" | "health" | "model" | "pricing" | "balance" | "policy";
  label: string;
  value: string;
  severity: "info" | "warning" | "error";
};

export type RoutingLatestDecisionView = {
  id: string;
  decidedAt: string;
  endpoint: RouteEndpointKind;
  model: string | null;
  selectedStationKeyId: string | null;
  selectedStationId: string | null;
  selectedStationName: string | null;
  policy: string;
  status: "selected" | "fallback" | "failed" | "unavailable";
  reason: string;
  fallbackCount: number;
};

export type RoutingCandidateView = {
  stationKeyId: string;
  stationId: string;
  stationName: string;
  keyName: string;
  endpoint: RouteEndpointKind;
  priority: number;
  enabled: boolean;
  schedulable: boolean;
  participationStatus: RoutingCandidateParticipationStatus;
  participationReason: RoutingCandidateParticipationReason;
  score: number | null;
  scoreDetails: RoutingWorkspaceCandidate["scoreDetails"];
  diagnostics?: RoutingWorkspaceCandidate["diagnostics"];
  currentConcurrency: number | null;
  lastSuccessAt: string | null;
  lastFailureAt: string | null;
  routingGroupScope: RoutingGroupFilter;
  routingGroupMatch: boolean;
  scoreStatus: RoutingScoreStatus;
  plannerExclusionCodes: string[];
  assessmentSnapshotId: string | null;
  assessmentDurableRevision: number | null;
  assessmentRequestContextFingerprint: string | null;
  facts: DecisionFact[];
  balanceValue?: number | null;
  balanceCurrency?: string | null;
};

export type RoutingWorkspaceView = {
  proxyStatus: ProxyStatus;
  settings: {
    enabled: boolean;
    bindAddr: string;
    port: number;
    endpoint: RouteEndpointKind;
    policy: RoutingWorkspaceSnapshot["policyConfig"];
    maxRateMultiplier: number | null;
    routingGroupFilter: RoutingGroupFilter;
    fallbackEnabled: boolean;
    previewKind: "baseline_eligibility";
    plannerEvaluation: RoutingPlannerEvaluationStatus;
    plannerEvaluationCode: string | null;
  };
  summary: {
    totalCandidateCount: number;
    currentPageCandidateCount: number;
    participatingCandidateCount: number;
    nonParticipatingCandidateCount: number;
    openCircuitCandidateCount: number;
    recoveryEligibleCandidateCount: number;
    readModelUnavailableCandidateCount: number;
    lastDecisionAt: string | null;
  };
  candidates: RoutingCandidateView[];
  latestDecision: RoutingLatestDecisionView | null;
};

function candidateFacts(candidate: RoutingWorkspaceCandidate, plannerExclusionCodes: string[]): DecisionFact[] {
  const facts: DecisionFact[] = [
    {
      kind: "capability",
      label: "Capability",
      value: candidate.capabilityVerdicts.protocol,
      severity: plannerExclusionCodes.length > 0 ? "warning" : "info",
    },
    {
      kind: "pricing",
      label: "Effective multiplier",
      value: candidate.multiplier.multiplier == null ? "unknown" : `${candidate.multiplier.multiplier}x`,
      severity: candidate.multiplier.ceilingRejected ? "warning" : "info",
    },
  ];
  if (candidate.balanceStatus) {
    facts.push({ kind: "balance", label: "Balance", value: candidate.balanceStatus, severity: "info" });
  }
  return facts;
}

export function toRoutingWorkspaceView(
  snapshot: RoutingWorkspaceSnapshot,
  proxyStatus: ProxyStatus,
  runtimeOverlay: RoutingRuntimeOverlay | null = null,
  latestDecision: RoutingLatestDecisionView | null = null,
): RoutingWorkspaceView {
  const runtimeByKey = new Map(
    (runtimeOverlay?.candidates ?? []).map((candidate) => [candidate.stationKeyId, candidate]),
  );
  const candidates = snapshot.candidates.map((candidate, index) => {
    const plannerExclusionCodes = [...candidate.plannerExclusionCodes];
    const scoreStatus = candidate.scoreStatus;
    const runtime = runtimeByKey.get(candidate.stationKeyId);
    const matchingRuntime =
      runtime?.stationId === candidate.stationId &&
      runtime.endpointRevision === candidate.endpointRevision
        ? runtime
        : null;
    return {
      stationKeyId: candidate.stationKeyId,
      stationId: candidate.stationId,
      stationName: candidate.stationName,
      keyName: candidate.keyName,
      endpoint: "chat_completions" as const,
      priority: candidate.priority || index,
      // Disabled stations/keys are excluded by the backend query. Do not
      // conflate that administrative state with request eligibility.
      enabled: true,
      schedulable: candidate.schedulable,
      participationStatus: candidate.participationStatus,
      participationReason: candidate.participationReason,
      score: candidate.score,
      scoreDetails: candidate.scoreDetails,
      diagnostics: candidate.diagnostics ?? null,
      currentConcurrency:
        matchingRuntime?.stationKeyInFlight == null
          ? null
          : Math.max(0, Math.trunc(matchingRuntime.stationKeyInFlight)),
      lastSuccessAt: null,
      lastFailureAt: null,
      routingGroupScope: snapshot.routingGroupFilter,
      routingGroupMatch: !plannerExclusionCodes.includes("group_mismatch"),
      scoreStatus,
      plannerExclusionCodes,
      assessmentSnapshotId: candidate.assessmentSnapshotId ?? null,
      assessmentDurableRevision: candidate.assessmentDurableRevision ?? null,
      assessmentRequestContextFingerprint: candidate.assessmentRequestContextFingerprint ?? null,
      facts: candidateFacts(candidate, plannerExclusionCodes),
      balanceValue: candidate.balanceValue,
      balanceCurrency: candidate.balanceCurrency,
    } satisfies RoutingCandidateView;
  });

  return {
    proxyStatus,
    settings: {
      enabled: proxyStatus.running,
      bindAddr: proxyStatus.bindAddr,
      port: proxyStatus.port,
      endpoint: "chat_completions",
      policy: snapshot.policyConfig,
      maxRateMultiplier: snapshot.maxRateMultiplier,
      routingGroupFilter: snapshot.routingGroupFilter,
      fallbackEnabled: true,
      previewKind: "baseline_eligibility",
      plannerEvaluation: snapshot.plannerEvaluation ?? "unavailable",
      plannerEvaluationCode: snapshot.plannerEvaluationCode ?? null,
    },
    summary: {
      totalCandidateCount: snapshot.aggregates.totalCandidates,
      currentPageCandidateCount: snapshot.candidates.length,
      participatingCandidateCount:
        snapshot.aggregates.eligibleCandidates + snapshot.aggregates.conditionallyEligibleCandidates,
      nonParticipatingCandidateCount:
        snapshot.aggregates.excludedCandidates + snapshot.aggregates.unavailableCandidates,
      openCircuitCandidateCount: snapshot.aggregates.openCircuits,
      recoveryEligibleCandidateCount: snapshot.aggregates.conditionallyEligibleCandidates,
      readModelUnavailableCandidateCount: snapshot.aggregates.unavailableCandidates,
      lastDecisionAt: latestDecision?.decidedAt ?? null,
    },
    candidates,
    latestDecision,
  };
}
