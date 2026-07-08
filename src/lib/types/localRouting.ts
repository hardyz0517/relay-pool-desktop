import type { ProxyStatus } from "@/lib/types/proxy";
import type { RouteEndpointKind } from "@/lib/types/routing";

export type RouteHealthState = "ready" | "cooldown" | "degraded" | "offline" | "unknown";

export type DecisionFactKind =
  | "capability"
  | "health"
  | "model"
  | "pricing"
  | "balance"
  | "policy";

export type DecisionFact = {
  kind: DecisionFactKind;
  label: string;
  value: string;
  severity: "info" | "warning" | "error";
};

export type LocalRoutingSettings = {
  enabled: boolean;
  bindAddr: string;
  port: number;
  endpoint: RouteEndpointKind;
  policy: string;
  fallbackEnabled: boolean;
};

export type LocalRoutingSummary = {
  enabledCandidateCount: number;
  healthyCandidateCount: number;
  degradedCandidateCount: number;
  cooldownCandidateCount: number;
  lastDecisionAt: string | null;
};

export type LocalRoutingCandidateRow = {
  stationKeyId: string;
  stationId: string;
  stationName: string;
  keyName: string;
  endpoint: RouteEndpointKind;
  priority: number;
  enabled: boolean;
  healthState: RouteHealthState;
  lastSuccessAt: string | null;
  lastFailureAt: string | null;
  cooldownUntil: string | null;
  score: number | null;
  facts: DecisionFact[];
};

export type RouteDecisionSummary = {
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

export type RouteDecisionEvent = {
  id: string;
  decisionId: string;
  occurredAt: string;
  stationKeyId: string | null;
  stationId: string | null;
  accepted: boolean;
  facts: DecisionFact[];
  message: string;
};

export type LocalRoutingWorkspace = {
  proxyStatus: ProxyStatus;
  settings: LocalRoutingSettings;
  summary: LocalRoutingSummary;
  candidates: LocalRoutingCandidateRow[];
  latestDecision: RouteDecisionSummary | null;
  recentEvents: RouteDecisionEvent[];
};
