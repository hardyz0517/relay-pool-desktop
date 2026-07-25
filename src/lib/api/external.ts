import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";

export function openExternalUrl(url: string) {
  return getActiveBackendClient().stations.openStationWebsite(url);
}
