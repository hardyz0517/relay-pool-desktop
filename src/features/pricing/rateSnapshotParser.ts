import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";

export type RateMultiplierRow = {
  stationId: string;
  groupBindingId: string | null;
  groupName: string;
  multiplier: number | null;
  source: string;
  status: string;
  confidence: number;
  updatedAt: string;
};

export function rateRowsFromGroupFacts(
  bindings: StationGroupBinding[],
  records: GroupRateRecord[],
): RateMultiplierRow[] {
  const bindingRows = bindings
    .filter((binding) => binding.bindingKind === "station_group")
    .map((binding) => ({
      stationId: binding.stationId,
      groupBindingId: binding.id,
      groupName: binding.groupName,
      multiplier: binding.effectiveRateMultiplier,
      source: binding.rateSource ?? "binding",
      status: binding.bindingStatus,
      confidence: binding.confidence,
      updatedAt: binding.lastCheckedAt ?? binding.updatedAt,
    }));

  const recordRows = records.map((record) => ({
    stationId: record.stationId,
    groupBindingId: record.groupBindingId,
    groupName: record.groupName,
    multiplier: record.effectiveRateMultiplier,
    source: record.source,
    status: "history",
    confidence: record.confidence,
    updatedAt: record.checkedAt,
  }));

  return [...bindingRows, ...recordRows];
}
