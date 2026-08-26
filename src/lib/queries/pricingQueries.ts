import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import type {
  PricingGroupMonitorStatusInput,
  PricingGroupMonitorStatusWorkspace,
} from "@/lib/types/pricingMonitoring";

export type PricingComparisonWorkspace = {
  stations: Station[];
  stationKeys: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  developerModeEnabled: boolean;
};

export function loadPricingComparisonWorkspace(): Promise<PricingComparisonWorkspace> {
  return getActiveBackendClient().pricing.loadPricingComparisonWorkspace();
}

export function loadPricingGroupMonitorStatus(
  input: PricingGroupMonitorStatusInput,
): Promise<PricingGroupMonitorStatusWorkspace> {
  return getActiveBackendClient().pricing.loadPricingGroupMonitorStatus(input);
}
