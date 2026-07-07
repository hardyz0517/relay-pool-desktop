import {
  buildCurrentStationBalanceFacts,
  type StationBalanceCurrentFact,
} from "@/lib/projections/balanceFacts";
import {
  buildCurrentStationGroupFacts,
  isDisplayableStationGroupCurrentFact,
} from "@/lib/projections/groupFacts";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import type { StatusTone } from "@/components/ui";

export type RateChip = {
  label: string;
  value: string;
  tone: "neutral" | "good" | "warning";
};

export type StationAssetRow = {
  station: Station;
  enabledKeyCount: number;
  warningKeyCount: number;
  latestBalance: BalanceSnapshot | null;
  currentBalance: StationBalanceCurrentFact;
  latestSnapshot: CollectorSnapshot | null;
  riskEvents: ChangeEvent[];
  rateChips: RateChip[];
  participatesInRouting: boolean;
};

export function buildStationAssetRows({
  stations,
  keysByStation,
  balances,
  snapshotsByStation,
  groupBindingsByStation,
  groupRatesByStation,
  changes,
}: {
  stations: Station[];
  keysByStation: Map<string, StationKey[]>;
  balances: BalanceSnapshot[];
  snapshotsByStation: Map<string, CollectorSnapshot | null>;
  groupBindingsByStation: Map<string, StationGroupBinding[]>;
  groupRatesByStation?: Map<string, GroupRateRecord[]>;
  changes: ChangeEvent[];
}): StationAssetRow[] {
  const currentBalancesByStation = buildCurrentStationBalanceFacts({ stations, balances });
  return stations.map((station) => {
    const keys = keysByStation.get(station.id) ?? [];
    const currentBalance = currentBalancesByStation.get(station.id);
    const groupBindings = groupBindingsByStation.get(station.id) ?? [];
    const groupRates = groupRatesByStation?.get(station.id) ?? [];
    const riskEvents = changes.filter(
      (event) =>
        event.stationId === station.id &&
        event.status !== "dismissed" &&
        event.status !== "resolved" &&
        (event.severity === "critical" || event.severity === "warning"),
    );
    const enabledKeyCount = keys.length > 0 ? keys.filter((key) => key.enabled).length : station.keyCount;
    return {
      station,
      enabledKeyCount,
      warningKeyCount: keys.filter((key) => key.status === "warning" || key.status === "error").length,
      latestBalance: currentBalance?.sourceSnapshot ?? null,
      currentBalance: currentBalance ?? buildCurrentStationBalanceFacts({ stations: [station], balances: [] }).get(station.id)!,
      latestSnapshot: snapshotsByStation.get(station.id) ?? null,
      riskEvents,
      rateChips: rateChipsForStation(
        groupBindings,
        groupRates,
        snapshotsByStation.get(station.id) ?? null,
      ),
      participatesInRouting: station.enabled && enabledKeyCount > 0,
    };
  });
}

function rateChipsForStation(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
  snapshot: CollectorSnapshot | null,
): RateChip[] {
  const currentFactChips = rateChipsFromCurrentFacts(bindings, rates);
  return currentFactChips.length > 0 ? currentFactChips : extractRateChips(snapshot);
}

export function rateChipsFromCurrentFacts(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
): RateChip[] {
  return buildCurrentStationGroupFacts({ bindings, rates })
    .filter(isDisplayableStationGroupCurrentFact)
    .slice(0, 3)
    .map((fact) => ({
      label: fact.groupName,
      value: typeof fact.rateMultiplier === "number" ? `${fact.rateMultiplier.toFixed(2)}x` : "-",
      tone: fact.rateMultiplier == null ? "warning" : fact.rateMultiplier < 1 ? "good" : "neutral",
    }));
}

export function extractRateChips(snapshot: CollectorSnapshot | null): RateChip[] {
  const rates = Array.isArray(snapshot?.normalizedJson.rateMultipliers)
    ? (snapshot?.normalizedJson.rateMultipliers as Array<Record<string, unknown>>)
    : [];
  return rates.slice(0, 3).map((rate) => {
    const group = String(rate.groupName ?? rate.group ?? rate.name ?? "default");
    const multiplier = Number(rate.multiplier ?? rate.rate ?? rate.value ?? 1);
    return {
      label: group,
      value: Number.isFinite(multiplier) ? `${multiplier.toFixed(2)}x` : "-",
      tone: !Number.isFinite(multiplier) ? "neutral" : multiplier > 1 ? "warning" : multiplier < 1 ? "good" : "neutral",
    };
  });
}

export function formatStationBalance(row: StationAssetRow) {
  const value = row.currentBalance.value;
  if (value == null) {
    return "未采集";
  }
  return `${row.currentBalance.currency} ${value.toFixed(2)}`;
}

export function stationRiskTone(row: StationAssetRow): StatusTone {
  if (!row.station.enabled) {
    return "disabled";
  }
  if (row.riskEvents.some((event) => event.severity === "critical")) {
    return "error";
  }
  if (row.riskEvents.some((event) => event.severity === "warning") || row.warningKeyCount > 0) {
    return "warning";
  }
  if (row.station.status === "healthy") {
    return "healthy";
  }
  if (row.station.status === "error") {
    return "error";
  }
  if (row.station.status === "warning") {
    return "warning";
  }
  return "info";
}

