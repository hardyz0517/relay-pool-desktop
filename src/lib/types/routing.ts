export type RoutingPolicy =
  | "automatic_balanced"
  | "priority_fallback"
  | "stable_first"
  | "backup_only"
  | "cheap_first"
  | "cost_stable_first";
export type RouteEndpointKind = "models" | "chat_completions" | "responses" | "embeddings";

export type PricingGroupType = "gpt" | "claude" | "gemini" | "grok" | "image_generation";

export type RoutingGroupFilter =
  | "all_groups"
  | "ungrouped_only"
  | { group_binding_id: string }
  | { group_id_hash: string }
  | { group_type: PricingGroupType };

export type SchedulerAdvancedSettings = {
  topK: number;
  multiplier: number;
  priority: number;
  load: number;
  queue: number;
  errorRate: number;
  ttft: number;
  quotaHeadroom: number;
  previousResponse: number;
  sessionSticky: number;
  multiplierMinConfidence: number;
  stickyWeighted: boolean;
  stickyEscape: boolean;
  stickyEscapeTtftMs: number;
  stickyEscapeErrorRate: number;
  stickySessionTtlSeconds: number;
  stickyResponseTtlSeconds: number;
  stickyMaxWaiting: number;
  stickyWaitTimeoutSeconds: number;
  fallbackMaxWaiting: number;
  fallbackWaitTimeoutSeconds: number;
};

export type StationKeyCapabilities = {
  stationKeyId: string;
  supportsChatCompletions: boolean;
  supportsResponses: boolean;
  supportsEmbeddings: boolean;
  supportsStream: boolean;
  supportsTools: boolean;
  supportsVision: boolean;
  supportsReasoning: boolean;
  modelAllowlist: string[];
  modelBlocklist: string[];
  preferredModels: string[];
  onlyUseAsBackup: boolean;
  routingTags: string[];
  updatedAt: string;
};

export type UpdateStationKeyCapabilitiesInput = Omit<StationKeyCapabilities, "updatedAt">;

export type ModelAlias = {
  id: string;
  clientModel: string;
  upstreamModel: string;
  enabled: boolean;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type UpsertModelAliasInput = {
  id: string | null;
  clientModel: string;
  upstreamModel: string;
  enabled: boolean;
  note: string | null;
};

export type StationKeyHealth = {
  stationKeyId: string;
  lastSuccessAt: string | null;
  lastFailureAt: string | null;
  consecutiveFailures: number;
  successCount: number;
  failureCount: number;
  avgLatencyMs: number | null;
  lastErrorSummary: string | null;
  cooldownUntil: string | null;
  updatedAt: string;
};

export type RouteSimulationInput = {
  endpoint: RouteEndpointKind;
  model: string | null;
  stream: boolean;
  usesTools: boolean;
  usesVision: boolean;
  usesReasoning: boolean;
  policy: RoutingPolicy | null;
  maxRateMultiplier?: number | null;
  routingGroupFilter?: RoutingGroupFilter | null;
  sessionHash?: string | null;
  previousResponseId?: string | null;
};

export type RouteCandidateExplanation = {
  stationKeyId: string;
  stationId: string;
  stationName: string;
  keyName: string;
  accepted: boolean;
  score: number;
  reasons: string[];
  rejectionReasons: string[];
  mappedModel: string | null;
  pricingRuleId: string | null;
  groupBindingId: string | null;
  rateMultiplier: number | null;
  normalizationStatus: string | null;
  priceConfidence: number | null;
  estimatedInputPrice: number | null;
  estimatedOutputPrice: number | null;
  priceCurrency: string | null;
  balanceStatus: string | null;
  balanceValue: number | null;
  balanceScope: string | null;
  balanceCollectedAt: string | null;
  economicFreshness: string | null;
  economicReasons: string[];
  routingGroupScope: RoutingGroupFilter | null;
  routingGroupMatch: boolean;
  groupIdHash: string | null;
  groupType: PricingGroupType | null;
  effectiveMultiplierSource: string | null;
  effectiveMultiplierConfidence: number | null;
  schedulerScore: number | null;
  schedulerFactors: string[];
  topKRank: number | null;
  slotResult: string | null;
};

export type RouteSimulationResult = {
  previewPolicyVersion: string;
  capacityMode: string;
  selectedCapacityAcquired: boolean;
  selectedStationKeyId: string | null;
  selectedStationId: string | null;
  mappedModel: string | null;
  policy: RoutingPolicy;
  maxRateMultiplier: number | null;
  routingGroupFilter: RoutingGroupFilter;
  schedulerErrorCode: string | null;
  candidates: RouteCandidateExplanation[];
  message: string;
};

export type RoutingWorkspaceSnapshotInput = {
  limit?: number | null;
  cursor?: string | null;
};

export type RoutingCapacityReadMode = "snapshot_only";
export type RoutingReadModelStatus = "available" | "unavailable";

export type RoutingCapabilitySummary = {
  chatCompletions: boolean;
  responses: boolean;
  embeddings: boolean;
  stream: boolean;
  tools: boolean;
  vision: boolean;
  reasoning: boolean;
};

export type RoutingCandidateCapacitySnapshot = {
  mode: RoutingCapacityReadMode;
  maxConcurrency: number;
  inFlight: number | null;
  acquired: boolean;
};

export type RoutingCandidateSourceRefs = {
  stationKeyId: string;
  stationId: string;
  endpointRevision: number;
};

export type RoutingWorkspaceCandidate = {
  stationKeyId: string;
  stationId: string;
  stationName: string;
  keyName: string;
  endpointRevision: number;
  priority: number;
  schedulable: boolean;
  healthState: string;
  capabilitySummary: RoutingCapabilitySummary;
  priceBasis: string;
  balanceStatus: string | null;
  capacity: RoutingCandidateCapacitySnapshot;
  sourceRefs: RoutingCandidateSourceRefs;
};

export type RoutingReadPage = {
  limit: number;
  returned: number;
  nextCursor: string | null;
};

export type RoutingWorkspaceSnapshot = {
  readModelVersion: string;
  generatedAtMs: number;
  productionPolicy: RoutingPolicy;
  previewPolicyVersion: string;
  maxRateMultiplier: number | null;
  routingGroupFilter: RoutingGroupFilter;
  capacityMode: RoutingCapacityReadMode;
  page: RoutingReadPage;
  candidates: RoutingWorkspaceCandidate[];
  readModelStatus: RoutingReadModelStatus;
};

export type RoutingRuntimeCandidateOverlay = {
  stationKeyId: string;
  stationId: string;
  endpointRevision: number;
  inFlight: number | null;
  healthState: string;
  cooldownUntil: string | null;
};

export type RoutingRuntimeOverlay = {
  overlayVersion: string;
  sampledAtMs: number;
  revision: number;
  candidates: RoutingRuntimeCandidateOverlay[];
};

export type RecentRouteDecisionsInput = {
  limit?: number | null;
  cursor?: string | null;
};

export type RouteDecisionSummary = {
  requestLogId: string;
  requestId: string | null;
  createdAt: string;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  endpoint: string;
  model: string | null;
  status: string;
  lifecycleStatus: string | null;
  stationKeyId: string | null;
  stationId: string | null;
  routePolicy: string | null;
  routeReason: string | null;
  fallbackCount: number;
  costStatus: string | null;
  estimatedTotalCost: number | null;
  costCurrency: string | null;
};

export type RecentRouteDecisionsPage = {
  pageVersion: string;
  decisions: RouteDecisionSummary[];
  nextCursor: string | null;
  readModelStatus: RoutingReadModelStatus;
};

export type OperationalDetailFact = {
  scope: string;
  name: string;
  value: string;
  source: string;
  freshness: string;
  reason: string | null;
};

export type StationKeyOperationalDetail = {
  detailVersion: string;
  stationKeyId: string;
  stationId: string;
  endpointRevision: number;
  facts: OperationalDetailFact[];
  lazyHistoryAvailable: boolean;
  readModelStatus: RoutingReadModelStatus;
};

export type RequestDecisionTrace = {
  traceVersion: string;
  requestLogId: string;
  status: "legacy_summary" | "trace_unavailable";
  reason: string;
  legacySummary: {
    routePolicy: string | null;
    routeReason: string | null;
    stationKeyId: string | null;
    stationId: string | null;
    fallbackCount: number;
  } | null;
  planningRounds: unknown[];
};
