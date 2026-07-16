import { invoke } from "@tauri-apps/api/core";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";

export function openExternalUrl(url: string) {
  return invoke<void>("open_external_url", { url }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
    throw error;
  });
}
