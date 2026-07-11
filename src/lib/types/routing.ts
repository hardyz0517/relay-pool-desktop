export type RoutingPolicy =
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
};

export type RouteSimulationResult = {
  selectedStationKeyId: string | null;
  selectedStationId: string | null;
  mappedModel: string | null;
  policy: RoutingPolicy;
  candidates: RouteCandidateExplanation[];
  message: string;
};
