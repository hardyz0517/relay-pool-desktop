import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, CcswitchImportResult, UpdateSettingsInput } from "@/lib/types/settings";

export const SETTINGS_UPDATED_EVENT = "relay-pool-settings-updated";

let memorySettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "cost_stable_first",
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  balanceIntervalMinutes: 5,
  groupRateIntervalMinutes: 20,
  modelListIntervalMinutes: 60,
  pricingRefreshIntervalMinutes: 60,
  collectorTimeoutSeconds: 15,
  collectorMaxConcurrency: 3,
  allowDepletedFallback: false,
  trayBehavior: "minimize-to-tray",
  developerModeEnabled: false,
  dataDir: "仅桌面端可读取",
  pendingDataDir: null,
  dataDirChangeRequiresRestart: false,
};

export function getSettings() {
  return invoke<AppSettings>("get_settings").then(normalizeSettings).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return normalizeSettings(memorySettings);
    }
    throw error;
  });
}

export function getLocalAccessKey() {
  return invoke<string>("get_local_access_key").catch((error) => {
    if (isInvokeUnavailable(error)) {
      throw new Error("只有桌面端可以复制真实本地访问密钥");
    }
    throw error;
  });
}

export function importRelayPoolToCCSwitch() {
  return invoke<CcswitchImportResult>("import_relay_pool_to_ccswitch").catch((error) => {
    if (isInvokeUnavailable(error)) {
      throw new Error("只有桌面端可以导入 CCSwitch");
    }
    throw error;
  });
}

export function updateSettings(input: UpdateSettingsInput) {
  return invoke<AppSettings>("update_settings", { input }).then(normalizeSettings).catch((error) => {
    if (isInvokeUnavailable(error)) {
      memorySettings = { ...memorySettings, ...input };
      return normalizeSettings(memorySettings);
    }
    throw error;
  });
}

export function chooseDataDir() {
  return invoke<AppSettings>("choose_data_dir").then(normalizeSettings).catch((error) => {
    if (isInvokeUnavailable(error)) {
      throw new Error("只有桌面端可以选择数据保存位置");
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

function normalizeRoutingStrategy(value: AppSettings["defaultRoutingStrategy"] | string) {
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
  return "cost_stable_first";
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}

function normalizeBoolean(value: unknown) {
  return value === true || value === "true" || value === 1 || value === "1";
}

function normalizeNumber(value: unknown, fallback: number) {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : fallback;
}
