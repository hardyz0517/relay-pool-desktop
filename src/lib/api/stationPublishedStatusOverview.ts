import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { StationPublishedStatusOverview, StationPublishedStatusOverviewInput } from "@/lib/types/stationPublishedStatus";

export function getStationPublishedStatusOverview(input: StationPublishedStatusOverviewInput = {}): Promise<StationPublishedStatusOverview> {
  const client = getActiveBackendClient().stationPublishedStatus;
  if (!client?.getStationPublishedStatusOverview) {
    return Promise.reject(new Error("Official status overview is not available."));
  }
  return client.getStationPublishedStatusOverview(input);
}
