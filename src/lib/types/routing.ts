export type RoutingPolicy = "priority_fallback" | "stable_first" | "backup_only";
export type RouteEndpointKind = "models" | "chat_completions" | "responses" | "embeddings";

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
