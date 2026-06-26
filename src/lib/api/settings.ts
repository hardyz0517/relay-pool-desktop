import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, UpdateSettingsInput } from "@/lib/types/settings";

let memorySettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "manual",
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  trayBehavior: "minimize-to-tray",
  dataDir: "等待 Tauri 数据目录",
};

export function getSettings() {
  return invoke<AppSettings>("get_settings").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memorySettings;
    }
    throw error;
  });
}

export function updateSettings(input: UpdateSettingsInput) {
  return invoke<AppSettings>("update_settings", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      memorySettings = { ...memorySettings, ...input };
      return memorySettings;
    }
    throw error;
  });
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
