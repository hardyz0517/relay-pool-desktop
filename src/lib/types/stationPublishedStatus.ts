export type StationPublishedStatusSourceState =
  | "never_collected"
  | "available"
  | "empty"
  | "unsupported"
  | "authorization_required"
  | "degraded"
  | "failed";

export type StationPublishedStatusCompleteness = "complete" | "partial";
export type StationPublishedStatusOutcome = "available" | "degraded" | "unavailable" | "unknown";
export type StationPublishedStatusIdentityKind = "upstream_id" | "derived_fallback";

export type StationPublishedStatusSample = {
  id: string;
  model: string;
  outcome: StationPublishedStatusOutcome;
  latencyMs: number | null;
  pingLatencyMs: number | null;
  checkedAtMs: number;
};

export type StationPublishedStatusRow = {
  rowKey: string;
  upstreamMonitorId: string;
  identityKind: StationPublishedStatusIdentityKind;
  name: string;
  provider: string;
  groupName: string | null;
  primaryModel: string;
  extraModels: string[];
  currentOutcome: StationPublishedStatusOutcome;
  currentLatencyMs: number | null;
  currentPingLatencyMs: number | null;
  recentAvailabilityPercent: number | null;
  upstreamCheckedAtMs: number | null;
  recentSamples: StationPublishedStatusSample[];
};

export type StationPublishedStatusWorkspace = {
  stationId: string;
  endpointRevision: number;
  supported: boolean;
  sourceState: StationPublishedStatusSourceState;
  completeness: StationPublishedStatusCompleteness | null;
  lastAttemptAtMs: number | null;
  lastSuccessAtMs: number | null;
  lastCompleteAtMs: number | null;
  monitorCount: number;
  stale: boolean;
  safeErrorKind: string | null;
  rows: StationPublishedStatusRow[];
};

export type StationPublishedStatusOverviewInput = {
  filter?: { search?: string | null; stationId?: string | null; outcome?: StationPublishedStatusOutcome | null; sourceState?: StationPublishedStatusSourceState | null };
  cursor?: string | null;
  limit?: number;
};

export type StationPublishedStatusOverviewRow = StationPublishedStatusRow & {
  stationId: string;
  stationName: string;
  stationType: string;
  stationEnabled: boolean;
  stationPriority: number;
  endpointRevision: number;
  sourceKind: string;
  sourceState: StationPublishedStatusSourceState;
  completeness: StationPublishedStatusCompleteness | null;
  stale: boolean;
  lastAttemptAtMs: number | null;
  lastSuccessAtMs: number | null;
  lastCompleteAtMs: number | null;
};

export type StationPublishedStatusOverview = {
  readAtMs: number;
  summary: Record<string, number>;
  rows: StationPublishedStatusOverviewRow[];
  page: { limit: number; returned: number; nextCursor: string | null };
};
