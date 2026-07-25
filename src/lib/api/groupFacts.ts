import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { UpsertStationGroupBindingInput } from "@/lib/types/groupFacts";

export function listStationGroupBindings(stationId: string) {
  return getActiveBackendClient().groupFacts.listStationGroupBindings(stationId);
}

export function listStationGroupOptions(stationId: string) {
  return getActiveBackendClient().groupFacts.listStationGroupOptions(stationId);
}

export function listGroupRateRecords(stationId: string) {
  return getActiveBackendClient().groupFacts.listGroupRateRecords(stationId);
}

export function upsertStationGroupBinding(input: UpsertStationGroupBindingInput) {
  return getActiveBackendClient().groupFacts.upsertStationGroupBinding(input);
}
