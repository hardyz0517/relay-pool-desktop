import type { CollectorSnapshot } from "@/lib/types/collector";

export type RateMultiplierRow = {
  stationId: string;
  groupName: string;
  multiplier: number | null;
  source: string;
  updatedAt: string;
};

export function parseRateMultipliers(snapshot: CollectorSnapshot | null): RateMultiplierRow[] {
  if (!snapshot) {
    return [];
  }
  const rawRates = Array.isArray(snapshot.normalizedJson.rateMultipliers)
    ? (snapshot.normalizedJson.rateMultipliers as Array<Record<string, unknown>>)
    : [];
  return rawRates.map((rate) => {
    const multiplier = Number(rate.multiplier ?? rate.rate ?? rate.value);
    return {
      stationId: snapshot.stationId,
      groupName: String(rate.groupName ?? rate.group ?? rate.name ?? "default"),
      multiplier: Number.isFinite(multiplier) ? multiplier : null,
      source: snapshot.source,
      updatedAt: snapshot.fetchedAt,
    };
  });
}
