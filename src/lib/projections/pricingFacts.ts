// RPD_ROUTING_BOUNDARY:display-only-routing-truth-compat
// UI-only compatibility projection for pricing/group pages. Production routing
// truth is owned by backend operational read models and routing projectors.
import { deriveStationGroupDisplayFacts, type StationGroupCurrentFact } from "@/lib/projections/groupFacts";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type PricingGroupCandidate = {
  identityKey: string;
  station: Station;
  stationKeyId: string | null;
  stationKeyName: string | null;
  groupBindingId: string | null;
  groupRateRecordId: string | null;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  groupRawJsonRedacted: Record<string, unknown> | null;
  groupMultiplier: number | null;
  source: string;
  checkedAt: string | null;
  currentFact: StationGroupCurrentFact;
};

export function derivePricingGroupDisplayCandidates(input: {
  stations: Station[];
  stationKeys?: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
}): PricingGroupCandidate[] {
  const stationsById = new Map(input.stations.map((station) => [station.id, station]));
  const stationKeyNameById = new Map((input.stationKeys ?? []).map((key) => [key.id, key.name]));

  return deriveStationGroupDisplayFacts({
    bindings: input.groupBindings,
    rates: input.groupRates,
  })
    .filter((fact) => fact.available && fact.bindingKind === "station_group")
    .flatMap((fact) => {
      const station = stationsById.get(fact.stationId);
      if (!station) {
        return [];
      }
      const groupMultiplier = fact.rateMultiplier;
      const stationKeyId = fact.stationKeyId ?? null;
      return [
        {
          identityKey: fact.identityKey,
          station,
          stationKeyId,
          stationKeyName: stationKeyId ? stationKeyNameById.get(stationKeyId) ?? null : null,
          groupBindingId: fact.groupBindingId,
          groupRateRecordId: fact.rateEvidenceId,
          groupKeyHash: fact.groupKeyHash ?? "",
          groupIdHash: fact.groupIdHash,
          groupName: fact.groupName,
          groupRawJsonRedacted: fact.sourceRate?.rawJsonRedacted ?? fact.sourceBinding?.rawJsonRedacted ?? null,
          groupMultiplier,
          source: fact.rateSource ?? "station_group_current_fact",
          checkedAt: fact.rateCheckedAt,
          currentFact: fact,
        },
      ];
    });
}
