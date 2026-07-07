import { toTimestampMillis } from "@/lib/time";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { StationGroupBinding } from "@/lib/types/groupFacts";
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
  changes,
}: {
  stations: Station[];
  keysByStation: Map<string, StationKey[]>;
  balances: BalanceSnapshot[];
  snapshotsByStation: Map<string, CollectorSnapshot | null>;
  groupBindingsByStation: Map<string, StationGroupBinding[]>;
  changes: ChangeEvent[];
}): StationAssetRow[] {
  const latestBalanceByStation = latestBalanceMap(balances);
  return stations.map((station) => {
    const keys = keysByStation.get(station.id) ?? [];
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
      latestBalance: latestBalanceByStation.get(station.id) ?? null,
      latestSnapshot: snapshotsByStation.get(station.id) ?? null,
      riskEvents,
      rateChips: rateChipsForStation(
        groupBindingsByStation.get(station.id) ?? [],
        snapshotsByStation.get(station.id) ?? null,
      ),
      participatesInRouting: station.enabled && enabledKeyCount > 0,
    };
  });
}

function rateChipsForStation(
  bindings: StationGroupBinding[],
  snapshot: CollectorSnapshot | null,
): RateChip[] {
  const durableChips = rateChipsFromBindings(bindings);
  return durableChips.length > 0 ? durableChips : extractRateChips(snapshot);
}

export function rateChipsFromBindings(bindings: StationGroupBinding[]): RateChip[] {
  return bindings
    .filter((binding) => binding.bindingKind === "station_group")
    .slice(0, 3)
    .map((binding) => {
      const multiplier = binding.effectiveRateMultiplier ?? binding.defaultRateMultiplier;
      return {
        label: binding.groupName,
        value: typeof multiplier === "number" ? `${multiplier.toFixed(2)}x` : "-",
        tone: binding.bindingStatus === "missing" ? "warning" : "neutral",
      };
    });
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
  const value = row.latestBalance?.value ?? row.station.balanceCny;
  if (value == null) {
    return "未采集";
  }
  const currency = row.latestBalance?.currency ?? "CNY";
  return `${currency} ${value.toFixed(2)}`;
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

function latestBalanceMap(balances: BalanceSnapshot[]) {
  const map = new Map<string, BalanceSnapshot>();
  for (const balance of balances) {
    if (balance.scope !== "station") {
      continue;
    }
    const current = map.get(balance.stationId);
    if (!current || toTime(balance.updatedAt) > toTime(current.updatedAt)) {
      map.set(balance.stationId, balance);
    }
  }
  return map;
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const time = toTimestampMillis(value);
  return Number.isNaN(time) ? 0 : time;
}
