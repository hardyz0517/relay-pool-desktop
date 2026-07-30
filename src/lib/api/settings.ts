import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  UpdateSettingsInput,
  UpsertCommonLoginEmailInput,
  UpsertCommonLoginPasswordInput,
} from "@/lib/types/settings";

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

export function listCommonLoginOptions() {
  return getActiveBackendClient().settings.listCommonLoginOptions();
}

export function upsertCommonLoginEmail(input: UpsertCommonLoginEmailInput) {
  return getActiveBackendClient().settings.upsertCommonLoginEmail(input);
}

export function deleteCommonLoginEmail(id: string) {
  return getActiveBackendClient().settings.deleteCommonLoginEmail(id);
}

export function upsertCommonLoginPassword(input: UpsertCommonLoginPasswordInput) {
  return getActiveBackendClient().settings.upsertCommonLoginPassword(input);
}

export function deleteCommonLoginPassword(id: string) {
  return getActiveBackendClient().settings.deleteCommonLoginPassword(id);
}

export function getCommonLoginPassword(id: string) {
  return getActiveBackendClient().settings.getCommonLoginPassword(id);
}
