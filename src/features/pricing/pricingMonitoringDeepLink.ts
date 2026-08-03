import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";
import type { PricingComparisonRow } from "./pricingComparisonViewModel";

export function buildPricingMonitoringDeepLink(
  row: Pick<PricingComparisonRow, "stationId" | "monitorSummary">,
): RoutingDeepLink | null {
  const stationKeyId = row.monitorSummary?.representativeKeyId?.trim();
  if (stationKeyId) {
    return {
      kind: "station-key",
      stationKeyId,
      source: "pricing",
    };
  }
  const stationId = row.stationId.trim();
  return stationId
    ? {
        kind: "station",
        stationId,
        source: "pricing",
      }
    : null;
}
