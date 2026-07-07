import { buildCurrentStationGroupFacts, type StationGroupCurrentFact } from "@/lib/projections/groupFacts";
import type { PricingRule } from "@/lib/types/economics";
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
  pricingRuleId: string | null;
  source: string;
  checkedAt: string | null;
  currentFact: StationGroupCurrentFact;
};

export function buildPricingGroupCandidates(input: {
  stations: Station[];
  stationKeys?: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
}): PricingGroupCandidate[] {
  const stationsById = new Map(input.stations.map((station) => [station.id, station]));
  const stationKeyNameById = new Map((input.stationKeys ?? []).map((key) => [key.id, key.name]));
  const pricingRules = input.pricingRules.filter((rule) => rule.enabled);

  return buildCurrentStationGroupFacts({
    bindings: input.groupBindings,
    rates: input.groupRates,
  })
    .filter((fact) => fact.available && fact.bindingKind === "station_group")
    .flatMap((fact) => {
      const station = stationsById.get(fact.stationId);
      if (!station) {
        return [];
      }
      const matchingRule = firstMatchingPricingRule(fact, pricingRules);
      const ruleMultiplier = firstFiniteNumber(matchingRule?.rateMultiplier);
      const groupMultiplier = fact.rateMultiplier ?? ruleMultiplier;
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
          pricingRuleId: fact.rateMultiplier === null ? matchingRule?.id ?? null : null,
          source: fact.rateSource ?? matchingRule?.source ?? "station_group_current_fact",
          checkedAt: fact.rateCheckedAt ?? matchingRule?.collectedAt ?? null,
          currentFact: fact,
        },
      ];
    });
}

function firstMatchingPricingRule(
  fact: StationGroupCurrentFact,
  pricingRules: PricingRule[],
) {
  return pricingRules.find((rule) => {
    if (rule.stationId !== fact.stationId) {
      return false;
    }
    if (fact.groupBindingId && rule.groupBindingId) {
      return rule.groupBindingId === fact.groupBindingId;
    }
    if (rule.groupName && normalizedName(rule.groupName) === normalizedName(fact.groupName)) {
      return true;
    }
    return false;
  }) ?? null;
}

function firstFiniteNumber(...values: Array<number | null | undefined>) {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
  }
  return null;
}

function normalizedName(value: string) {
  return value.trim().toLowerCase();
}
