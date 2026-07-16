import { invoke } from "@tauri-apps/api/core";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import {
  DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  SCHEDULER_ADVANCED_FIELD_KINDS,
  type AppSettings,
  type CcswitchImportResult,
  type UpdateSettingsInput,
} from "@/lib/types/settings";
import type { SchedulerAdvancedSettings } from "@/lib/types/routing";

export const SETTINGS_UPDATED_EVENT = "relay-pool-settings-updated";

let memorySettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "automatic_balanced",
  collectorProxyMode: "direct",
  collectorProxyUrl: null,
  maxRateMultiplier: null,
  defaultRoutingGroupFilter: "all_groups",
  schedulerAdvancedSettings: DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  balanceIntervalMinutes: 5,
  groupRateIntervalMinutes: 20,
  modelListIntervalMinutes: 60,
  pricingRefreshIntervalMinutes: 60,
  collectorTimeoutSeconds: 15,
  collectorMaxConcurrency: 3,
  allowDepletedFallback: false,
  developerModeEnabled: false,
  dataDir: "仅桌面端可读取",
  pendingDataDir: null,
  dataDirChangeRequiresRestart: false,
};

export function getSettings() {
  return invoke<AppSettings>("get_settings").then(normalizeSettings).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return normalizeSettings(memorySettings);
    }
    throw error;
  });
}

export function getLocalAccessKey() {
  return invoke<string>("get_local_access_key").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      throw new Error("只有桌面端可以复制真实本地访问密钥");
    }
    throw error;
  });
}

export function updateLocalAccessKey(value: string) {
  return invoke<AppSettings>("update_local_access_key", { value }).then(normalizeSettings).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      const localKeyMasked = maskSecret(value);
      memorySettings = { ...memorySettings, localKeyMasked };
      return normalizeSettings(memorySettings);
    }
    throw error;
  });
}

export function importRelayPoolToCCSwitch() {
  return invoke<CcswitchImportResult>("import_relay_pool_to_ccswitch").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      throw new Error("只有桌面端可以导入 CCSwitch");
    }
    throw error;
  });
}

export function updateSettings(input: UpdateSettingsInput) {
  return invoke<AppSettings>("update_settings", { input }).then(normalizeSettings).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      memorySettings = { ...memorySettings, ...input };
      return normalizeSettings(memorySettings);
    }
    throw error;
  });
}

export function chooseDataDir() {
  return invoke<AppSettings>("choose_data_dir").then(normalizeSettings).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      throw new Error("只有桌面端可以选择数据保存位置");
    }
    throw error;
  });
}

export function resetDataDir() {
  return invoke<AppSettings>("reset_data_dir").then(normalizeSettings).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      throw new Error("只有桌面端可以恢复默认数据保存位置");
    }
    throw error;
  });
}

function normalizeSettings(settings: AppSettings): AppSettings {
  const maybeSettings = settings as AppSettings & Partial<Record<keyof AppSettings, unknown>>;
  return {
    ...settings,
    pendingDataDir: typeof maybeSettings.pendingDataDir === "string" ? maybeSettings.pendingDataDir : null,
    dataDirChangeRequiresRestart: normalizeBoolean(maybeSettings.dataDirChangeRequiresRestart),
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

function maskSecret(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return "未设置";
  }
  if (trimmed.length <= 8) {
    return "****";
  }
  return `${trimmed.slice(0, 4)}****${trimmed.slice(-4)}`;
}

function normalizeNullableNumber(value: unknown) {
  if (value === null || value === undefined || value === "") {
    return null;
  }
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : null;
}
