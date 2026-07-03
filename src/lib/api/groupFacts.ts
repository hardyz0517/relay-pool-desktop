import { invoke } from "@tauri-apps/api/core";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";

const memoryBindings = new Map<string, StationGroupBinding[]>();
const memoryRates = new Map<string, GroupRateRecord[]>();

export function listStationGroupBindings(stationId: string) {
  return invoke<StationGroupBinding[]>("list_station_group_bindings", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryBindings.get(stationId) ?? [];
    }
    throw error;
  });
}

export function listGroupRateRecords(stationId: string) {
  return invoke<GroupRateRecord[]>("list_group_rate_records", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryRates.get(stationId) ?? [];
    }
    throw error;
  });
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
