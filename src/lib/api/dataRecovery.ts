import { invoke } from "@tauri-apps/api/core";

import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { ActivationResult, DataStoreCandidate, DataStoreStartupView } from "@/lib/types/dataRecovery";

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

export function refreshDataStoreCandidates() {
  return invoke<DataStoreStartupView>("refresh_data_store_candidates");
}

export function locateDataStoreCandidate() {
  return invoke<DataStoreCandidate | null>("locate_data_store_candidate");
}

export function createNewDataStore(confirmed: boolean) {
  return invoke<ActivationResult>("create_new_data_store", { confirmed });
}

export function openDataStoreBackupDir() {
  return invoke<void>("open_data_store_backup_dir");
}

export function exportDataStoreDiagnostic() {
  return invoke<string | null>("export_data_store_diagnostic");
}
