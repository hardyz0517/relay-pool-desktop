import { openExternalUrl as openExternalUrlBinding } from "@/lib/bridge/generated";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";

export function openExternalUrl(url: string) {
  return openExternalUrlBinding({ url }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
    throw error;
  });
}
