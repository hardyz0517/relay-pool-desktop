import { invoke } from "@tauri-apps/api/core";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { CollectorRun } from "@/lib/types/collectorRuns";

export function listCollectorRuns(stationId: string) {
  return invoke<CollectorRun[]>("list_collector_runs", { stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return [];
    }
    throw error;
  });
}
