import { invoke } from "@tauri-apps/api/core";
import { listPricingRules } from "@/lib/api/economics";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { getSettings } from "@/lib/api/settings";
import { listStationKeys } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import { isTauriCommandNotFound, isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type PricingComparisonWorkspace = {
  stations: Station[];
  stationKeys: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
  developerModeEnabled: boolean;
};

export async function loadPricingComparisonWorkspace(): Promise<PricingComparisonWorkspace> {
  try {
    return await invoke<PricingComparisonWorkspace>("load_pricing_comparison_workspace");
  } catch (error) {
    if (!isTauriInvokeUnavailable(error) && !isTauriCommandNotFound(error)) {
      throw error;
    }
    const [pricingRules, stations, settings] = await Promise.all([
      listPricingRules(),
      listStations(),
      getSettings(),
    ]);
    const [bindingLists, rateRecordLists, stationKeyLists] = await Promise.all([
      Promise.all(stations.map((station) => listStationGroupBindings(station.id))),
      Promise.all(stations.map((station) => listGroupRateRecords(station.id))),
      Promise.all(stations.map((station) => listStationKeys(station.id))),
    ]);

    return {
      stations,
      stationKeys: stationKeyLists.flat(),
      groupBindings: bindingLists.flat(),
      groupRates: rateRecordLists.flat(),
      pricingRules,
      developerModeEnabled: settings.developerModeEnabled,
    };
  }
}
