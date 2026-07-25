import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";

export function listCollectorRuns(stationId: string) {
  return getActiveBackendClient().collectorRuns.listCollectorRuns(stationId);
}
