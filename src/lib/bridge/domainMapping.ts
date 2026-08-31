import type {
  CreateStationInputDto,
  EndpointPingResultDto,
  SettingsDto,
  StationPublishedStatusWorkspaceDto,
  StationEndpointHealthDto,
  StationDto,
  UpdateSettingsInputDto,
  UpdateStationInputDto,
} from "./generated";
import type { AppSettings, UpdateSettingsInput } from "@/lib/types/settings";
import type {
  EndpointPingResult,
  Station,
  StationEndpointHealth,
  StationInput,
  StationUpdateInput,
} from "@/lib/types/stations";
import type {
  DataStoreCandidate,
  DataStoreStartupView,
  SchemaCompatibilityView,
} from "@/lib/types/dataRecovery";
import type { StationPublishedStatusOverview, StationPublishedStatusWorkspace } from "@/lib/types/stationPublishedStatus";
import type { StationPublishedStatusOverviewDto } from "./generated";

export function normalizeSettings(settings: SettingsDto | AppSettings): AppSettings {
  const maybeSettings = settings as SettingsDto & Partial<Record<keyof AppSettings, unknown>>;
  return {
    ...settings,
    pendingDataDir: typeof maybeSettings.pendingDataDir === "string" ? maybeSettings.pendingDataDir : null,
    dataDirChangeRequiresRestart: normalizeBoolean(maybeSettings.dataDirChangeRequiresRestart),
    localProxyStartOnLaunch: normalizeBoolean(maybeSettings.localProxyStartOnLaunch),
    collectorProxyMode: normalizeCollectorProxyMode(maybeSettings.collectorProxyMode),
    collectorProxyUrl:
      typeof maybeSettings.collectorProxyUrl === "string" && maybeSettings.collectorProxyUrl.trim()
        ? maybeSettings.collectorProxyUrl.trim()
        : null,
    balanceIntervalMinutes: normalizeNumber(maybeSettings.balanceIntervalMinutes, 5),
    groupRateIntervalMinutes: normalizeNumber(maybeSettings.groupRateIntervalMinutes, 20),
    publishedStatusIntervalMinutes: normalizeNumber(maybeSettings.publishedStatusIntervalMinutes, 5),
    pricingRefreshIntervalMinutes: normalizeNumber(maybeSettings.pricingRefreshIntervalMinutes, 60),
    collectorTimeoutSeconds: normalizeNumber(maybeSettings.collectorTimeoutSeconds, 60),
    collectorMaxConcurrency: normalizeNumber(maybeSettings.collectorMaxConcurrency, 3),
    developerModeEnabled: normalizeBoolean(
      maybeSettings.developerModeEnabled,
    ),
  };
}

export function toUpdateSettingsDto(input: UpdateSettingsInput): UpdateSettingsInputDto {
  return {
    localProxyPort: input.localProxyPort,
    collectorProxyMode: input.collectorProxyMode,
    collectorProxyUrl: input.collectorProxyUrl,
    lowBalanceThresholdCny: input.lowBalanceThresholdCny,
    collectorIntervalMinutes: input.collectorIntervalMinutes,
    balanceIntervalMinutes: input.balanceIntervalMinutes,
    groupRateIntervalMinutes: input.groupRateIntervalMinutes,
    publishedStatusIntervalMinutes: input.publishedStatusIntervalMinutes,
    pricingRefreshIntervalMinutes: input.pricingRefreshIntervalMinutes,
    collectorTimeoutSeconds: input.collectorTimeoutSeconds,
    collectorMaxConcurrency: input.collectorMaxConcurrency,
    developerModeEnabled: input.developerModeEnabled,
  };
}

export function normalizeStation(station: StationDto): Station {
  return {
    ...station,
    stationType: station.stationType,
    collectorProxyMode:
      station.collectorProxyMode === "direct" ||
      station.collectorProxyMode === "system" ||
      station.collectorProxyMode === "manual"
        ? station.collectorProxyMode
        : "inherit",
    status:
      station.status === "healthy" ||
      station.status === "warning" ||
      station.status === "error" ||
      station.status === "disabled"
        ? station.status
        : "unchecked",
  };
}

export function toCreateStationDto(input: StationInput): CreateStationInputDto {
  return {
    name: input.name,
    stationType: input.stationType,
    websiteUrl: input.websiteUrl,
    apiBaseUrl: input.apiBaseUrl,
    apiKey: input.apiKey,
    collectorProxyMode: input.collectorProxyMode,
    collectorProxyUrl: input.collectorProxyUrl,
    enabled: input.enabled,
    creditPerCny: input.creditPerCny,
    lowBalanceThresholdCny: input.lowBalanceThresholdCny,
    collectionIntervalMinutes: input.collectionIntervalMinutes,
    note: input.note,
  };
}

export function toUpdateStationDto(input: StationUpdateInput): UpdateStationInputDto {
  return {
    ...toCreateStationDto({ ...input, apiKey: input.apiKey ?? "" }),
    id: input.id,
    apiKey: input.apiKey,
  };
}

export function normalizeStationEndpointHealth(
  health: StationEndpointHealthDto,
): StationEndpointHealth {
  return {
    stationId: health.stationId,
    status:
      health.status === "success" || health.status === "failed"
        ? health.status
        : "unchecked",
    latencyMs: health.latencyMs,
    checkedAt: health.checkedAt,
    errorSummary: health.errorSummary,
    updatedAt: health.updatedAt,
  };
}

export function normalizeEndpointPingResult(result: EndpointPingResultDto): EndpointPingResult {
  return {
    ...result,
    status: result.status === "success" ? "success" : "failed",
  };
}

export function normalizeStationPublishedStatusWorkspace(
  workspace: StationPublishedStatusWorkspaceDto,
): StationPublishedStatusWorkspace {
  return {
    stationId: workspace.stationId,
    endpointRevision: workspace.endpointRevision,
    supported: workspace.supported,
    sourceState: workspace.sourceState,
    completeness: workspace.completeness,
    lastAttemptAtMs: workspace.lastAttemptAtMs,
    lastSuccessAtMs: workspace.lastSuccessAtMs,
    lastCompleteAtMs: workspace.lastCompleteAtMs,
    monitorCount: workspace.monitorCount,
    stale: workspace.stale,
    safeErrorKind: workspace.safeErrorKind,
    rows: workspace.rows.map((row) => ({
      rowKey: row.id,
      upstreamMonitorId: row.upstreamMonitorId,
      identityKind: row.identityKind,
      name: row.name,
      provider: row.provider,
      groupName: row.groupName,
      primaryModel: row.primaryModel,
      extraModels: row.extraModels,
      currentOutcome: row.currentOutcome,
      currentLatencyMs: row.currentLatencyMs,
      currentPingLatencyMs: row.currentPingLatencyMs,
      recentAvailabilityPercent: row.recentAvailabilityPercent,
      upstreamCheckedAtMs: row.upstreamCheckedAtMs,
      recentSamples: row.samples.flatMap((sample, index) => {
        return [{
          id: `${row.id}:${sample.model}:${sample.checkedAtMs}:${index}`,
          model: sample.model,
          outcome: sample.outcome,
          latencyMs: sample.latencyMs,
          pingLatencyMs: sample.pingLatencyMs,
          checkedAtMs: sample.checkedAtMs,
        }];
      }),
    })),
  };
}

export function normalizeStationPublishedStatusOverview(
  overview: StationPublishedStatusOverviewDto,
): StationPublishedStatusOverview {
  return {
    readAtMs: overview.readAtMs,
    summary: overview.summary,
    rows: overview.rows.map((row) => ({
      rowKey: row.id,
      stationId: row.stationId,
      stationName: row.stationName,
      stationType: row.stationType,
      stationEnabled: row.stationEnabled,
      stationPriority: row.stationPriority,
      endpointRevision: row.endpointRevision,
      sourceKind: row.sourceKind,
      sourceState: row.sourceState,
      completeness: row.completeness,
      stale: row.stale,
      lastAttemptAtMs: row.lastAttemptAtMs,
      lastSuccessAtMs: row.lastSuccessAtMs,
      lastCompleteAtMs: row.lastCompleteAtMs,
      upstreamMonitorId: row.upstreamMonitorId,
      identityKind: row.identityKind,
      name: row.name,
      provider: row.provider,
      groupName: row.groupName,
      primaryModel: row.primaryModel,
      extraModels: row.extraModels,
      currentOutcome: row.currentOutcome,
      currentLatencyMs: row.currentLatencyMs,
      currentPingLatencyMs: row.currentPingLatencyMs,
      recentAvailabilityPercent: row.recentAvailabilityPercent,
      upstreamCheckedAtMs: row.upstreamCheckedAtMs,
      recentSamples: row.samples.map((sample, index) => ({
        id: `${row.id}:${sample.model}:${sample.checkedAtMs}:${index}`,
        ...sample,
      })),
    })),
    page: overview.page,
  };
}

export function normalizeDataStoreStartupView(value: unknown): DataStoreStartupView {
  if (!isRecord(value) || !isRuntimeMode(value.mode) || !isGeneration(value.databaseGeneration)) {
    throw new Error("invalid data store startup response");
  }
  if (!isCapabilities(value.capabilities) || !Array.isArray(value.candidates)) {
    throw new Error("invalid data store startup response");
  }
  if (!(value.compatibility === null || isCompatibility(value.compatibility))) {
    throw new Error("invalid data store startup response");
  }
  if (!isUpgradeStatus(value.upgrade)) {
    throw new Error("invalid data store startup response");
  }
  if (!value.candidates.every(isCandidate)) {
    throw new Error("invalid data store startup response");
  }
  if (!isDecisionForMode(value.decision, value.mode)) {
    throw new Error("invalid data store startup response");
  }
  if (
    value.mode === "writable" &&
    (!isCompatibility(value.compatibility) || value.compatibility.decisionCode !== "writable")
  ) {
    throw new Error("invalid data store startup response");
  }
  if (value.mode === "inspectionOnly" && !isCompatibility(value.compatibility)) {
    throw new Error("invalid data store startup response");
  }
  return value as DataStoreStartupView;
}

export function normalizeDataStoreCandidate(value: unknown): DataStoreCandidate | null {
  if (value === null) return null;
  if (!isCandidate(value)) throw new Error("invalid data store candidate response");
  return value as DataStoreCandidate;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isRuntimeMode(value: unknown): value is DataStoreStartupView["mode"] {
  return value === "writable" || value === "inspectionOnly" || value === "recovery";
}

function isGeneration(value: unknown): value is DataStoreStartupView["databaseGeneration"] {
  return value === "two";
}

function isUpgradeStatus(value: unknown): boolean {
  if (!isRecord(value)) return false;
  const stage = value.stage;
  const current = value.currentSchemaVersion;
  const target = value.targetSchemaVersion;
  const failureReason = value.failureReason;
  const failureStage = value.failureStage;
  if (typeof stage !== "string" || !["probe", "migrate", "validate", "ready", "blocked"].includes(stage)) {
    return false;
  }
  if (!isSchemaVersion(current) || !isSchemaVersion(target) || target === null ||
      (current !== null && current > target)) {
    return false;
  }
  if (!(failureReason === null || isRecoveryReason(failureReason))) {
    return false;
  }
  const validFailureStage = typeof failureStage === "string" &&
    ["probe", "migrate", "validate"].includes(failureStage);
  return stage === "blocked"
    ? failureReason !== null && validFailureStage
    : failureReason === null && failureStage === null;
}

function isSchemaVersion(value: unknown): value is number | null {
  return value === null || (
    typeof value === "number" && Number.isSafeInteger(value) && value >= 0
  );
}

function isRecoveryReason(value: unknown): boolean {
  return [
    "missing", "unreadable", "invalidSqlite", "integrityFailed", "openOrMigrationFailed",
    "missingKey", "keyMismatch", "corruptedDatabase", "interruptedUpgrade",
    "schemaMigrationFailed", "alertingUpgradeFailed", "secretBaselineFailed", "internalUpgradeError",
    "unsupportedSchemaVersion", "inconsistentSchemaMetadata", "pendingRelocation",
    "systemCredentialMissing",
    "systemCredentialUnavailable", "systemCredentialPermissionDenied", "systemCredentialCorrupt",
    "systemCredentialUnsupported", "systemCredentialInternal",
    "portableMigrationManualRecoveryRequired", "portableMigrationKeyUnavailable",
  ].includes(String(value));
}

function isCapabilities(value: unknown): boolean {
  return isRecord(value) &&
    [
      "canBackup",
      "canExportDiagnostic",
      "canCheckForUpdates",
      "canLocateCandidate",
      "canActivateCandidate",
      "canCreateDataStore",
    ].every((key) => typeof value[key] === "boolean");
}

function isCandidate(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return typeof value.id === "string" &&
    ["active", "default", "source", "pending", "backup", "located"].includes(String(value.role)) &&
    typeof value.path === "string" &&
    ["healthy", "missing", "unreadable", "invalidSqlite", "integrityFailed"].includes(String(value.health)) &&
    (value.databaseGeneration === null || isGeneration(value.databaseGeneration)) &&
    (value.compatibility === null || isCompatibility(value.compatibility)) &&
    (typeof value.sizeBytes === "number" || value.sizeBytes === null) &&
    (typeof value.modifiedAt === "string" || value.modifiedAt === null) &&
    isRecord(value.counts) &&
    Object.values(value.counts).every((count) => typeof count === "number");
}

function isCompatibility(value: unknown): value is SchemaCompatibilityView {
  if (!isRecord(value)) return false;
  return isCompatibilityDecision(value.decisionCode) &&
    (value.schemaVersion === null || typeof value.schemaVersion === "number") &&
    typeof value.appVersion === "string";
}

function isCompatibilityDecision(value: unknown): boolean {
  return [
    "writable",
    "inspectionOnly",
    "generationMismatch",
    "readerTooOld",
    "writerTooOld",
    "metadataMismatch",
  ].includes(String(value));
}

function isDecisionForMode(value: unknown, mode: DataStoreStartupView["mode"]): boolean {
  if (!isRecord(value) || typeof value.kind !== "string") return false;
  if (mode === "writable") {
    return value.kind === "ready" && typeof value.candidateId === "string";
  }
  if (mode === "inspectionOnly") {
    return value.kind === "inspectionOnly" &&
      typeof value.candidateId === "string" &&
      isCompatibilityDecision(value.reason);
  }
  if (value.kind === "firstRun") return typeof value.defaultDataDir === "string";
  if (value.kind === "conflict") {
    return Array.isArray(value.candidateIds) &&
      value.candidateIds.every((candidateId) => typeof candidateId === "string");
  }
  return value.kind === "needsRecovery" &&
    [
      "missing",
      "unreadable",
      "invalidSqlite",
      "integrityFailed",
      "openOrMigrationFailed",
      "missingKey",
      "keyMismatch",
      "corruptedDatabase",
      "interruptedUpgrade",
      "schemaMigrationFailed",
      "alertingUpgradeFailed",
      "secretBaselineFailed",
      "internalUpgradeError",
      "unsupportedSchemaVersion",
      "inconsistentSchemaMetadata",
      "pendingRelocation",
      "systemCredentialMissing",
      "systemCredentialUnavailable",
      "systemCredentialPermissionDenied",
      "systemCredentialCorrupt",
      "systemCredentialUnsupported",
      "systemCredentialInternal",
      "portableMigrationManualRecoveryRequired",
      "portableMigrationKeyUnavailable",
    ].includes(String(value.reason));
}

function normalizeCollectorProxyMode(value: unknown): AppSettings["collectorProxyMode"] {
  if (value === "system" || value === "manual") {
    return value;
  }
  return "direct";
}

function normalizeBoolean(value: unknown) {
  return value === true || value === "true" || value === 1 || value === "1";
}

function normalizeNumber(value: unknown, fallback: number) {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : fallback;
}
