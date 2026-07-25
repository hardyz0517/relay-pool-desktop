import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { UpdateSettingsInput } from "@/lib/types/settings";

export const SETTINGS_UPDATED_EVENT = "relay-pool-settings-updated";

export function getSettings() {
  return getActiveBackendClient().settings.getSettings();
}

export function getLocalAccessKey() {
  return getActiveBackendClient().settings.getLocalAccessKey();
}

export function updateLocalAccessKey(value: string) {
  return getActiveBackendClient().settings.updateLocalAccessKey(value);
}

export function importRelayPoolToCCSwitch() {
  return getActiveBackendClient().settings.importRelayPoolToCCSwitch();
}

export function updateSettings(input: UpdateSettingsInput) {
  return getActiveBackendClient().settings.updateSettings(input);
}

export function chooseDataDir() {
  return getActiveBackendClient().settings.chooseDataDir();
}

export function resetDataDir() {
  return getActiveBackendClient().settings.resetDataDir();
}
