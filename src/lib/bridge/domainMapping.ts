import type {
  CreateStationInputDto,
  EndpointPingResultDto,
  SettingsDto,
  StationEndpointHealthDto,
  StationDto,
  UpdateSettingsInputDto,
  UpdateStationInputDto,
} from "./generated";
import {
  DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  SCHEDULER_ADVANCED_FIELD_KINDS,
  type AppSettings,
  type UpdateSettingsInput,
} from "@/lib/types/settings";
import type { SchedulerAdvancedSettings } from "@/lib/types/routing";
import type {
  EndpointPingResult,
  Station,
  StationEndpointHealth,
  StationInput,
  StationUpdateInput,
} from "@/lib/types/stations";

export function normalizeSettings(settings: SettingsDto | AppSettings): AppSettings {
  const maybeSettings = settings as SettingsDto & Partial<Record<keyof AppSettings, unknown>>;
  return {
    ...settings,
    pendingDataDir: typeof maybeSettings.pendingDataDir === "string" ? maybeSettings.pendingDataDir : null,
    dataDirChangeRequiresRestart: normalizeBoolean(maybeSettings.dataDirChangeRequiresRestart),
    localProxyStartOnLaunch: normalizeBoolean(maybeSettings.localProxyStartOnLaunch),
    defaultRoutingStrategy: normalizeRoutingStrategy(settings.defaultRoutingStrategy),
    collectorProxyMode: normalizeCollectorProxyMode(maybeSettings.collectorProxyMode),
    collectorProxyUrl:
      typeof maybeSettings.collectorProxyUrl === "string" && maybeSettings.collectorProxyUrl.trim()
        ? maybeSettings.collectorProxyUrl.trim()
        : null,
    maxRateMultiplier: normalizeNullableNumber(maybeSettings.maxRateMultiplier),
    defaultRoutingGroupFilter: maybeSettings.defaultRoutingGroupFilter ?? "all_groups",
    schedulerAdvancedSettings: normalizeSchedulerAdvancedSettings(
      maybeSettings.schedulerAdvancedSettings,
    ),
    balanceIntervalMinutes: normalizeNumber(maybeSettings.balanceIntervalMinutes, 5),
    groupRateIntervalMinutes: normalizeNumber(maybeSettings.groupRateIntervalMinutes, 20),
    modelListIntervalMinutes: normalizeNumber(maybeSettings.modelListIntervalMinutes, 60),
    pricingRefreshIntervalMinutes: normalizeNumber(maybeSettings.pricingRefreshIntervalMinutes, 60),
    collectorTimeoutSeconds: normalizeNumber(maybeSettings.collectorTimeoutSeconds, 15),
    collectorMaxConcurrency: normalizeNumber(maybeSettings.collectorMaxConcurrency, 3),
    developerModeEnabled: normalizeBoolean(
      maybeSettings.developerModeEnabled,
    ),
    allowDepletedFallback: normalizeBoolean(
      maybeSettings.allowDepletedFallback,
    ),
  };
}

export function toUpdateSettingsDto(input: UpdateSettingsInput): UpdateSettingsInputDto {
  return {
    localProxyPort: input.localProxyPort,
    defaultRoutingStrategy: input.defaultRoutingStrategy,
    collectorProxyMode: input.collectorProxyMode,
    collectorProxyUrl: input.collectorProxyUrl,
    maxRateMultiplier: input.maxRateMultiplier,
    defaultRoutingGroupFilter: input.defaultRoutingGroupFilter,
    schedulerAdvancedSettings: input.schedulerAdvancedSettings,
    lowBalanceThresholdCny: input.lowBalanceThresholdCny,
    collectorIntervalMinutes: input.collectorIntervalMinutes,
    balanceIntervalMinutes: input.balanceIntervalMinutes,
    groupRateIntervalMinutes: input.groupRateIntervalMinutes,
    modelListIntervalMinutes: input.modelListIntervalMinutes,
    pricingRefreshIntervalMinutes: input.pricingRefreshIntervalMinutes,
    collectorTimeoutSeconds: input.collectorTimeoutSeconds,
    collectorMaxConcurrency: input.collectorMaxConcurrency,
    allowDepletedFallback: input.allowDepletedFallback,
    developerModeEnabled: input.developerModeEnabled,
  };
}

export function normalizeStation(station: StationDto): Station {
  return {
    ...station,
    stationType:
      station.stationType === "sub2api" ||
      station.stationType === "newapi" ||
      station.stationType === "openai-compatible"
        ? station.stationType
        : "custom",
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

function normalizeSchedulerAdvancedSettings(value: unknown): SchedulerAdvancedSettings {
  const source = isRecord(value) ? value : {};
  const normalized: Record<string, number | boolean> = {
    ...DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  };

  for (const [key, kind] of Object.entries(SCHEDULER_ADVANCED_FIELD_KINDS)) {
    const fallback = DEFAULT_SCHEDULER_ADVANCED_SETTINGS[key as keyof SchedulerAdvancedSettings];
    if (kind === "boolean") {
      normalized[key] = normalizeBooleanWithFallback(source[key], Boolean(fallback));
      continue;
    }
    normalized[key] = normalizeSchedulerNumber(key, kind, source[key], Number(fallback));
  }

  const baseWeightFields = [
    "multiplier",
    "priority",
    "load",
    "queue",
    "errorRate",
    "ttft",
    "quotaHeadroom",
  ] as const;
  if (baseWeightFields.every((key) => normalized[key] === 0)) {
    for (const key of baseWeightFields) {
      normalized[key] = DEFAULT_SCHEDULER_ADVANCED_SETTINGS[key];
    }
  }

  return normalized as SchedulerAdvancedSettings;
}

function normalizeSchedulerNumber(
  key: string,
  kind: string,
  value: unknown,
  fallback: number,
) {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) {
    return fallback;
  }
  if (kind === "positiveInteger") {
    if (!Number.isSafeInteger(numeric) || numeric <= 0 || (key === "topK" && numeric > 65_535)) {
      return fallback;
    }
    return numeric;
  }
  if (kind === "ratio") {
    return numeric >= 0 && numeric <= 1 ? numeric : fallback;
  }
  return numeric >= 0 ? numeric : fallback;
}

function normalizeBooleanWithFallback(value: unknown, fallback: boolean) {
  if (value === true || value === "true" || value === 1 || value === "1") {
    return true;
  }
  if (value === false || value === "false" || value === 0 || value === "0") {
    return false;
  }
  return fallback;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeCollectorProxyMode(value: unknown): AppSettings["collectorProxyMode"] {
  if (value === "system" || value === "manual") {
    return value;
  }
  return "direct";
}

function normalizeRoutingStrategy(value: AppSettings["defaultRoutingStrategy"] | string) {
  if (value === "automatic" || value === "automatic_balanced") {
    return "automatic_balanced";
  }
  if (value === "stable" || value === "stable_first") {
    return "stable_first";
  }
  if (value === "backup_only") {
    return "backup_only";
  }
  if (value === "cheap_first") {
    return "cheap_first";
  }
  if (value === "cost_stable_first") {
    return "cost_stable_first";
  }
  return "automatic_balanced";
}

function normalizeBoolean(value: unknown) {
  return value === true || value === "true" || value === 1 || value === "1";
}

function normalizeNumber(value: unknown, fallback: number) {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : fallback;
}

function normalizeNullableNumber(value: unknown) {
  if (value === null || value === undefined || value === "") {
    return null;
  }
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : null;
}
