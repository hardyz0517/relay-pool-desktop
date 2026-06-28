import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, UpdateSettingsInput } from "@/lib/types/settings";

let memorySettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "priority_fallback",
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  trayBehavior: "minimize-to-tray",
  dataDir: "等待 Tauri 数据目录",
};

export function getSettings() {
  return invoke<AppSettings>("get_settings").then(normalizeSettings).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return normalizeSettings(memorySettings);
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

function normalizeSettings(settings: AppSettings): AppSettings {
  return {
    ...settings,
    defaultRoutingStrategy: normalizeRoutingStrategy(settings.defaultRoutingStrategy),
  };
}

function normalizeRoutingStrategy(value: AppSettings["defaultRoutingStrategy"] | string) {
  if (value === "stable" || value === "stable_first") {
    return "stable_first";
  }
  if (value === "backup_only") {
    return "backup_only";
  }
  return "priority_fallback";
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
