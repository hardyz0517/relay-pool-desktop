export type ChannelMonitorTargetType = "station_key" | "station";
export type ChannelMonitorProtocolKind = "open_ai_chat" | "open_ai_responses" | "anthropic_messages" | "gemini_native" | "xai_grok" | "generic_open_ai";
export type ChannelMonitorClientProfileId = "standard_api" | "codex_cli_compat" | "claude_code_compat" | "gemini_cli_compat" | "grok_cli_compat";
export type ChannelMonitorHealthWritebackMode = "disabled" | "observe_only" | "authoritative";

export type ChannelMonitorRunStatus = "success" | "warning" | "failed" | "skipped";

export type ChannelMonitorRequestTemplate = {
  id: string;
  name: string;
  endpointKind: string;
  method: string;
  path: string;
  requestBodyJson: string;
  enabled: boolean;
  builtIn: boolean;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CreateChannelMonitorTemplateInput = {
  name: string;
  endpointKind: string;
  method: string;
  path: string;
  requestBodyJson: string;
  enabled: boolean;
  note: string | null;
};

export type UpdateChannelMonitorTemplateInput = CreateChannelMonitorTemplateInput & {
  id: string;
};

export type ChannelMonitor = {
  id: string;
  name: string;
  targetType: ChannelMonitorTargetType;
  stationId: string;
  stationKeyId: string | null;
  templateId: string;
  enabled: boolean;
  protocolKind: ChannelMonitorProtocolKind;
  clientProfileId: ChannelMonitorClientProfileId;
  clientProfileVersion: number;
  primaryModel: string;
  retryMaxAttemptsPerModel: number;
  retryInitialBackoffMs: number;
  retryMaxBackoffMs: number;
  riskDailyProbeBudget: number;
  healthWritebackMode: ChannelMonitorHealthWritebackMode;
  healthFailureThreshold: number;
  healthRecoveryThreshold: number;
  attemptTimeoutMs: number;
  executionTimeoutMs: number;
  scheduleRevision: number;
  intervalSeconds: number;
  jitterSeconds: number;
  timeoutSeconds: number;
  maxConcurrency: number;
  consecutiveFailureThreshold: number;
  fallbackModels: string[];
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CreateChannelMonitorInput = {
  name: string;
  targetType: ChannelMonitorTargetType;
  stationId: string;
  stationKeyId: string | null;
  templateId: string;
  enabled: boolean;
  protocolKind: ChannelMonitorProtocolKind;
  clientProfileId: ChannelMonitorClientProfileId;
  clientProfileVersion: number;
  primaryModel: string;
  retryMaxAttemptsPerModel: number;
  retryInitialBackoffMs: number;
  retryMaxBackoffMs: number;
  riskDailyProbeBudget: number;
  healthWritebackMode: ChannelMonitorHealthWritebackMode;
  healthFailureThreshold: number;
  healthRecoveryThreshold: number;
  attemptTimeoutMs: number;
  executionTimeoutMs: number;
  intervalSeconds: number;
  jitterSeconds: number;
  timeoutSeconds: number;
  maxConcurrency: number;
  consecutiveFailureThreshold: number;
  fallbackModels: string[];
  note: string | null;
};

export type UpdateChannelMonitorInput = CreateChannelMonitorInput & {
  id: string;
};

export type ChannelMonitorRun = {
  id: string;
  monitorId: string;
  templateId: string;
  stationId: string;
  stationKeyId: string | null;
  status: ChannelMonitorRunStatus;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  httpStatus: number | null;
  latencyMs: number | null;
  responseModel: string | null;
  fallbackModel: string | null;
  errorMessage: string | null;
  createdAt: string;
};

export type ChannelMonitorRunsLoadStatus = "ok" | "failed";

export type ChannelMonitorSummary = {
  monitor: ChannelMonitor;
  recentRuns: ChannelMonitorRun[];
  runsLoadStatus: ChannelMonitorRunsLoadStatus;
  latestRun: ChannelMonitorRun | null;
};

export type ChannelStatusTimelinePoint = {
  status: ChannelMonitorRunStatus;
  latencyMs: number | null;
  endpointPingMs: number | null;
  checkedAt: string;
};

export type ChannelStatusWindowSummary = {
  window: "recent" | "24h" | "7d";
  totalCount: number;
  successCount: number;
  failureCount: number;
  warningCount: number;
  availabilityPercent: number | null;
  avgLatencyMs: number | null;
  avgEndpointPingMs: number | null;
  lastCheckedAt: string | null;
  latestStatus: ChannelMonitorRunStatus | null;
  latestErrorMessage: string | null;
  timeline: ChannelStatusTimelinePoint[];
};

export type ChannelStatusSummary = {
  monitor: ChannelMonitor;
  recent: ChannelStatusWindowSummary;
  last24h: ChannelStatusWindowSummary;
  last7d: ChannelStatusWindowSummary;
};

export type ChannelStatusWorkspaceWindow = "recent" | "last24h" | "last7d" | "last30d";
export type ChannelStatusSortField = "monitor_name" | "latest_checked_at" | "availability" | "latency" | "status";
export type ChannelStatusSortDirection = "asc" | "desc";
export type ChannelStatusOutcome = "available" | "degraded" | "unavailable" | "skipped" | "missing";
export type ChannelStatusBucketKind = "hour" | "day";
export type ChannelStatusBucketState = "missing" | "dirty" | "skipped_only" | "available" | "degraded" | "unavailable";

export type ChannelStatusCursor = {
  rowKey: string;
};

export type ChannelStatusFilter = {
  search?: string | null;
  enabled?: boolean | null;
  outcome?: ChannelStatusOutcome | null;
  stationId?: string | null;
  protocolKind?: string | null;
  clientProfileId?: string | null;
};

export type ChannelStatusSort = {
  field?: ChannelStatusSortField;
  direction?: ChannelStatusSortDirection;
};

export type ChannelStatusWorkspaceInput = {
  window?: ChannelStatusWorkspaceWindow;
  timezoneId?: string | null;
  filter?: ChannelStatusFilter;
  sort?: ChannelStatusSort;
  cursor?: ChannelStatusCursor | null;
  limit?: number | null;
};

export type ChannelStatusTimezone = {
  id: string;
  source: "iana" | "utc_fallback";
  requestedId: string | null;
};

export type ChannelStatusBucketBoundary = {
  kind: ChannelStatusBucketKind;
  startMs: number;
  endMs: number;
  label: string;
};

export type ChannelStatusBucketLayout = {
  recentLimit: number;
  hourly: ChannelStatusBucketBoundary[];
  daily: ChannelStatusBucketBoundary[];
};

export type ChannelStatusBucketCounts = {
  total: number;
  available: number;
  degraded: number;
  unavailable: number;
  skipped: number;
};

export type ChannelStatusBucket = {
  kind: ChannelStatusBucketKind;
  startMs: number;
  endMs: number;
  state: ChannelStatusBucketState;
  counts: ChannelStatusBucketCounts;
  strictAvailabilityBps: number | null;
  effectiveAvailabilityBps: number | null;
  p50LatencyMs: number | null;
  p95LatencyMs: number | null;
  failureCounts: Record<string, number>;
  dirty: boolean;
  corrupt: boolean;
};

export type ChannelStatusMonitor = {
  id: string;
  name: string;
  targetType: string;
  enabled: boolean;
  protocolKind: string;
  clientProfileId: string;
  clientProfileVersion: number;
  primaryModel: string;
  fallbackModels: string[];
  intervalSeconds: number;
  jitterSeconds: number;
  nextDueAtMs: number | null;
};

export type ChannelStatusTarget = {
  stationId: string;
  stationName: string | null;
  stationKeyId: string | null;
  stationKeyName: string | null;
  groupName: string | null;
  effectiveGroupCategory: string | null;
};

export type ChannelStatusLatestResult = {
  targetResultId: string;
  executionId: string;
  outcome: ChannelStatusOutcome;
  failureKind: string | null;
  terminalReason: string | null;
  latencyMs: number | null;
  finishedAtMs: number | null;
  semanticConfidence: string;
  usedFallback: boolean;
  attemptCount: number;
  effectiveModel: string | null;
};

export type ChannelStatusRunningExecution = {
  executionId: string;
  status: string;
  triggerKind: string;
  triggerRequestId: string | null;
  plannedAtMs: number;
  startedAtMs: number | null;
};

export type ChannelStatusRecentPoint = {
  targetResultId: string;
  executionId: string;
  outcome: ChannelStatusOutcome;
  failureKind: string | null;
  terminalReason: string | null;
  latencyMs: number | null;
  checkedAtMs: number | null;
  usedFallback: boolean;
  semanticConfidence: string;
  attemptCount: number;
  effectiveModel: string | null;
};

export type ChannelStatusWindowSummaryV2 = {
  window: ChannelStatusWorkspaceWindow;
  bucketKind: ChannelStatusBucketKind;
  startMs: number;
  endMs: number;
  counts: ChannelStatusBucketCounts;
  strictAvailabilityBps: number | null;
  effectiveAvailabilityBps: number | null;
  latestOutcome: ChannelStatusOutcome;
  latestCheckedAtMs: number | null;
  dirty: boolean;
  corrupt: boolean;
};

export type ChannelStatusRow = {
  rowKey: string;
  monitor: ChannelStatusMonitor;
  target: ChannelStatusTarget;
  latest: ChannelStatusLatestResult | null;
  running: ChannelStatusRunningExecution | null;
  recent: ChannelStatusRecentPoint[];
  hourlyBuckets: ChannelStatusBucket[];
  dailyBuckets: ChannelStatusBucket[];
  selectedWindow: ChannelStatusWindowSummaryV2;
};

export type ChannelStatusAggregate = {
  totalRows: number;
  returnedRows: number;
  runningRows: number;
  availableRows: number;
  degradedRows: number;
  unavailableRows: number;
  skippedRows: number;
  missingRows: number;
  dirtyRows: number;
};

export type ChannelStatusFreshness = {
  newestResultAtMs: number | null;
  oldestResultAtMs: number | null;
  hasDirtyRollups: boolean;
  hasCorruptRollups: boolean;
  runningExecutionCount: number;
};

export type ChannelStatusWorkspace = {
  schemaVersion: number;
  generatedAtMs: number;
  window: ChannelStatusWorkspaceWindow;
  timezone: ChannelStatusTimezone;
  bucketLayout: ChannelStatusBucketLayout;
  aggregate: ChannelStatusAggregate;
  freshness: ChannelStatusFreshness;
  page: {
    limit: number;
    returned: number;
    nextCursor: ChannelStatusCursor | null;
  };
  rows: ChannelStatusRow[];
};

export type RunChannelMonitorNowInput = {
  monitorId: string;
  triggerRequestId: string | null;
};

export type RunChannelMonitorReceipt = {
  executionId: string;
  monitorId: string;
  status: string;
  triggerRequestId: string;
  reusedExisting: boolean;
};

export type CancelChannelMonitorExecutionReceipt = {
  executionId: string;
  status: string;
  cancelled: boolean;
};

export type ChannelMonitorExecutionCursor = {
  startedAtMs: number;
  executionId: string;
};

export type ChannelMonitorExecutionListInput = {
  monitorId?: string | null;
  stationKeyId?: string | null;
  status?: string | null;
  cursor?: ChannelMonitorExecutionCursor | null;
  limit?: number | null;
};

export type ChannelMonitorExecutionSummaryV2 = {
  executionId: string;
  monitorId: string;
  status: string;
  triggerKind: string;
  triggerRequestId: string | null;
  plannedAtMs: number;
  startedAtMs: number | null;
  finishedAtMs: number | null;
  targetCount: number;
  availableCount: number;
  degradedCount: number;
  unavailableCount: number;
  skippedCount: number;
  summaryOutcome: string | null;
  summaryFailureKind: string | null;
  createdAtMs: number;
};

export type ChannelMonitorExecutionPage = {
  items: ChannelMonitorExecutionSummaryV2[];
  nextCursor: ChannelMonitorExecutionCursor | null;
};

export type ChannelMonitorTargetResultRecord = {
  targetResultId: string;
  executionId: string;
  monitorId: string;
  stationId: string;
  stationKeyId: string | null;
  terminalOutcome: string;
  terminalFailureKind: string | null;
  terminalReason: string | null;
  requestedModel: string;
  effectiveModel: string | null;
  usedFallback: boolean;
  attemptCount: number;
  decisiveAttemptId: string | null;
  protocolKind: string | null;
  resolvedAdapterKind: string;
  resolvedDialect: string | null;
  clientProfileId: string;
  clientProfileVersion: number;
  requestProfileHash: string | null;
  trafficEquivalence: string;
  healthWritebackMode: string;
  healthWritebackDecision: string;
  healthWritebackReason: string | null;
  latencyMs: number | null;
  semanticConfidence: string;
  startedAtMs: number;
  finishedAtMs: number | null;
};

export type ChannelMonitorExecutionDetail = {
  execution: ChannelMonitorExecutionSummaryV2;
  targets: ChannelMonitorTargetResultRecord[];
};

export type ChannelMonitorAttemptCursor = {
  startedAtMs: number;
  attemptId: string;
};

export type ChannelMonitorAttemptHistoryInput = {
  executionId: string;
  stationKeyId?: string | null;
  cursor?: ChannelMonitorAttemptCursor | null;
  limit?: number | null;
};

export type ChannelMonitorAttemptRecord = {
  attemptId: string;
  executionId: string;
  monitorId: string;
  stationId: string;
  stationKeyId: string | null;
  model: string;
  modelRole: string;
  modelIndex: number;
  attemptNumber: number;
  protocolKind: string;
  clientProfileId: string;
  clientProfileVersion: number;
  requestProfileHash: string;
  transportMode: string;
  startedAtMs: number;
  finishedAtMs: number | null;
  latencyMs: number | null;
  httpStatus: number | null;
  outcome: string;
  failureKind: string | null;
  retryable: boolean;
  responseModel: string | null;
  contentExtracted: boolean;
  validationPassed: boolean;
  outputBytes: number;
  errorSummary: string | null;
};

export type ChannelMonitorAttemptPage = {
  items: ChannelMonitorAttemptRecord[];
  nextCursor: ChannelMonitorAttemptCursor | null;
};

export type MonitoringProtocolCapability = {
  id: string;
  enabled: boolean;
  streaming: boolean;
};

export type MonitoringClientProfileCapability = {
  id: string;
  version: number;
  enabled: boolean;
  cliCompat: boolean;
  supportedProtocols: string[];
  method: string;
  path: string;
  headerNames: string[];
  bodyDefaults: string[];
  profileHash: string;
};

export type MonitoringCapabilityCatalog = {
  protocols: MonitoringProtocolCapability[];
  profiles: MonitoringClientProfileCapability[];
};
