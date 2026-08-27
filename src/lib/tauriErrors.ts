import { isTauri } from "@tauri-apps/api/core";

export function isTauriInvokeUnavailable(_error: unknown) {
  return !isTauri();
}
