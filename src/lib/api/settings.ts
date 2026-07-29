import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { UpdateSettingsInput, UpsertCommonLoginProfileInput } from "@/lib/types/settings";

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

export function listCommonLoginProfiles() {
  return getActiveBackendClient().settings.listCommonLoginProfiles();
}

export function upsertCommonLoginProfile(input: UpsertCommonLoginProfileInput) {
  return getActiveBackendClient().settings.upsertCommonLoginProfile(input);
}

export function deleteCommonLoginProfile(id: string) {
  return getActiveBackendClient().settings.deleteCommonLoginProfile(id);
}

export function getCommonLoginProfilePassword(id: string) {
  return getActiveBackendClient().settings.getCommonLoginProfilePassword(id);
}
