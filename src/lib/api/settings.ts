import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, UpdateSettingsInput } from "@/lib/types/settings";

export function getSettings() {
  return invoke<AppSettings>("get_settings");
}

export function updateSettings(input: UpdateSettingsInput) {
  return invoke<AppSettings>("update_settings", { input });
}
