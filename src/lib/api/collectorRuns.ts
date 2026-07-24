import { listCollectorRuns as listCollectorRunsGenerated } from "@/lib/bridge/generated";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";

export function listCollectorRuns(stationId: string) {
  return listCollectorRunsGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return [];
    }
    throw error;
  });
}
