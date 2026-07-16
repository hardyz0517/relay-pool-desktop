import { invoke } from "@tauri-apps/api/core";

import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { ActivationResult, DataStoreStartupView } from "@/lib/types/dataRecovery";

const browserPreviewStartupState: DataStoreStartupView = {
  decision: { kind: "ready", candidateId: "browser-preview" },
  candidates: [],
};

export async function getDataStoreStartupState(): Promise<DataStoreStartupView> {
  try {
    return await invoke<DataStoreStartupView>("get_data_store_startup_state");
  } catch (error) {
    if (isTauriInvokeUnavailable(error)) {
      return browserPreviewStartupState;
    }
    throw error;
  }
}

export function activateDataStoreCandidate(candidatePath: string) {
  return invoke<ActivationResult>("activate_data_store_candidate", { candidatePath });
}
