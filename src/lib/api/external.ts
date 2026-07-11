import { invoke } from "@tauri-apps/api/core";

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke/i.test(error.message);
}

export function openExternalUrl(url: string) {
  return invoke<void>("open_external_url", { url }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
    throw error;
  });
}
