import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { StationPublishedStatusWorkspace } from "@/lib/types/stationPublishedStatus";

export function getStationPublishedStatusWorkspace(
  stationId: string,
): Promise<StationPublishedStatusWorkspace> {
  const client = getActiveBackendClient().stationPublishedStatus;
  if (!client) {
    return Promise.resolve({
      stationId,
      endpointRevision: 0,
      supported: false,
      sourceState: "unsupported",
      completeness: null,
      lastAttemptAtMs: null,
      lastSuccessAtMs: null,
      lastCompleteAtMs: null,
      monitorCount: 0,
      stale: false,
      safeErrorKind: null,
      rows: [],
    });
  }
  return client.getStationPublishedStatusWorkspace(stationId);
}
