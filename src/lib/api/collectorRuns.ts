import { invoke } from "@tauri-apps/api/core";
import type { CollectorRun } from "@/lib/types/collectorRuns";

export function listCollectorRuns(stationId: string) {
  return invoke<CollectorRun[]>("list_collector_runs", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return [];
    }
    throw error;
  });
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
