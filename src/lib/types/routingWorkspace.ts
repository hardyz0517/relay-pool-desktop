import type { ProxyStatus } from "./proxy";
import type {
  RouteEndpointKind,
  RoutingGroupFilter,
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
  currentConcurrency: number | null;
  lastSuccessAt: string | null;
  lastFailureAt: string | null;
  cooldownUntil: string | null;
  routingGroupScope: RoutingGroupFilter;
  routingGroupMatch: boolean;
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

function candidateFacts(candidate: RoutingWorkspaceCandidate): DecisionFact[] {
  const facts: DecisionFact[] = [
    {
      kind: "capability",
      label: "Capability",
      value: candidate.capabilityVerdicts.protocol,
      severity: candidate.hardRejectionCodes.length > 0 ? "warning" : "info",
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
    const hardRejectionCodes = [...candidate.hardRejectionCodes];
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
      currentConcurrency:
        matchingRuntime?.stationKeyInFlight == null
          ? null
          : Math.max(0, Math.trunc(matchingRuntime.stationKeyInFlight)),
      lastSuccessAt: null,
      lastFailureAt: null,
      cooldownUntil: matchingRuntime?.cooldownUntil ?? null,
      routingGroupScope: snapshot.routingGroupFilter,
      routingGroupMatch: !hardRejectionCodes.includes("group_mismatch"),
      previewEligible: hardRejectionCodes.length === 0,
      previewRejectReasons: hardRejectionCodes,
      facts: candidateFacts(candidate),
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
    },
    summary: {
      candidateCount: candidates.length,
      previewEligibleCandidateCount: candidates.filter((candidate) => candidate.previewEligible).length,
      previewExcludedCandidateCount: candidates.filter((candidate) => !candidate.previewEligible).length,
      cooldownCandidateCount: candidates.filter((candidate) => candidate.healthState === "cooldown").length,
      lastDecisionAt: latestDecision?.decidedAt ?? null,
    },
    candidates,
    latestDecision,
  };
}
