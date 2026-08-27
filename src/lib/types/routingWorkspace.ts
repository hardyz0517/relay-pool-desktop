import type { ProxyStatus } from "./proxy";
import type {
  RouteEndpointKind,
  RoutingGroupFilter,
  RoutingPlannerEvaluationStatus,
  RoutingScoreStatus,
  RoutingRuntimeOverlay,
  RoutingWorkspaceCandidate,
  RoutingWorkspaceSnapshot,
} from "./routing";
export type RouteHealthState = "ready" | "cooldown" | "degraded" | "offline" | "unknown";
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
  healthState: RouteHealthState;
  score: number | null;
  scoreDetails: RoutingWorkspaceCandidate["scoreDetails"];
  currentConcurrency: number | null;
  lastSuccessAt: string | null;
  lastFailureAt: string | null;
  cooldownUntil: string | null;
  routingGroupScope: RoutingGroupFilter;
  routingGroupMatch: boolean;
  scoreStatus: RoutingScoreStatus;
  plannerExclusionCodes: string[];
  assessmentSnapshotId: string | null;
  assessmentDurableRevision: number | null;
  assessmentRequestContextFingerprint: string | null;
  previewEligible: boolean;
  previewRejectReasons: string[];
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
    candidateCount: number;
    previewEligibleCandidateCount: number;
    previewExcludedCandidateCount: number;
    cooldownCandidateCount: number;
    lastDecisionAt: string | null;
  };
  candidates: RoutingCandidateView[];
  latestDecision: RoutingLatestDecisionView | null;
};

const healthStates = new Set<RouteHealthState>(["ready", "cooldown", "degraded", "offline", "unknown"]);

function healthState(value: string): RouteHealthState {
  if (value === "available") return "ready";
  return healthStates.has(value as RouteHealthState) ? (value as RouteHealthState) : "unknown";
}

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
    const health = healthState(candidate.healthState);
    // Keep the DTO boundary tolerant of snapshots persisted before the
    // explicit planner status fields were introduced. New backend payloads
    // always provide these fields; this is only a one-way read compatibility
    // adapter for old fixtures/cache entries.
    const legacyCandidate = candidate as RoutingWorkspaceCandidate & {
      scoreStatus?: RoutingScoreStatus;
      plannerExclusionCodes?: string[];
      previewEligible?: boolean;
      previewRejectReasons?: string[];
    };
    const plannerExclusionCodes = [
      ...(legacyCandidate.plannerExclusionCodes ?? legacyCandidate.previewRejectReasons ?? []),
    ];
    const scoreStatus =
      legacyCandidate.scoreStatus ??
      (candidate.score != null || legacyCandidate.previewEligible
        ? "scored"
        : plannerExclusionCodes.length > 0
          ? "excluded"
          : "unavailable");
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
      healthState: health,
      score: candidate.score,
      scoreDetails: candidate.scoreDetails,
      currentConcurrency:
        matchingRuntime?.stationKeyInFlight == null
          ? null
          : Math.max(0, Math.trunc(matchingRuntime.stationKeyInFlight)),
      lastSuccessAt: null,
      lastFailureAt: null,
      cooldownUntil: matchingRuntime?.cooldownUntil ?? null,
      routingGroupScope: snapshot.routingGroupFilter,
      routingGroupMatch: !plannerExclusionCodes.includes("group_mismatch"),
      scoreStatus,
      plannerExclusionCodes,
      assessmentSnapshotId: candidate.assessmentSnapshotId ?? null,
      assessmentDurableRevision: candidate.assessmentDurableRevision ?? null,
      assessmentRequestContextFingerprint: candidate.assessmentRequestContextFingerprint ?? null,
      previewEligible: scoreStatus === "scored",
      previewRejectReasons: plannerExclusionCodes,
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
      candidateCount: candidates.length,
      previewEligibleCandidateCount: candidates.filter((candidate) => candidate.scoreStatus === "scored").length,
      previewExcludedCandidateCount: candidates.filter((candidate) => candidate.scoreStatus === "excluded").length,
      cooldownCandidateCount: candidates.filter((candidate) => candidate.healthState === "cooldown").length,
      lastDecisionAt: latestDecision?.decidedAt ?? null,
    },
    candidates,
    latestDecision,
  };
}
