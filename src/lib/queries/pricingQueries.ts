import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
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

export function loadPricingComparisonWorkspace(): Promise<PricingComparisonWorkspace> {
  return getActiveBackendClient().pricing.loadPricingComparisonWorkspace();
}
