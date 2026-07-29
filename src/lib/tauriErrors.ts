import { isTauri } from "@tauri-apps/api/core";

export function tauriErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function isTauriInvokeUnavailable(_error: unknown) {
  return !isTauri();
}
