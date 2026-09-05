import { BALANCE_CURRENCY } from "@/lib/balanceCurrency";
import {
  balanceSnapshotState,
  latestStationBalanceSnapshotsByStation,
} from "@/lib/projections/balanceFacts";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";

export type DashboardBalanceSummary = {
  latestStationBalances: BalanceSnapshot[];
  totalBalance: number;
  lowBalanceStations: number;
  unknownBalanceStations: number;
  staleBalanceStations: number;
  primaryBalanceCurrency: string | undefined;
  stationUsage: DashboardStationUsageSummary;
};

export type DashboardStationUsageSummary = {
  todayRequestCount: number;
  totalRequestCount: number;
  todayConsumption: number;
  totalConsumption: number;
  todayBaseConsumption: number | null;
  totalBaseConsumption: number | null;
  todayTokenCount: number;
  totalTokenCount: number;
  todayInputTokenCount: number;
  todayOutputTokenCount: number;
  totalInputTokenCount: number;
  totalOutputTokenCount: number;
};

export function summarizeDashboardBalances(
  balances: BalanceSnapshot[],
  stations: Array<Pick<Station, "id" | "creditPerCny">> = [],
): DashboardBalanceSummary {
  const creditPerCnyByStation = new Map(
    stations.map((station) => [station.id, safeCreditPerCny(station.creditPerCny)]),
  );

  const latestStationBalances = Array.from(latestStationBalanceSnapshotsByStation(balances).values());
  const states = latestStationBalances.map((snapshot) => balanceSnapshotState(snapshot));
  const currentBalances = latestStationBalances.filter((snapshot) => {
    const state = balanceSnapshotState(snapshot);
    return state === "available" || state === "depleted";
  });
  return {
    latestStationBalances,
    totalBalance: currentBalances.reduce((sum, snapshot) => sum + (snapshot.value ?? 0), 0),
    lowBalanceStations: currentBalances.filter(
      (snapshot) => snapshot.status === "low" || snapshot.status === "depleted",
    ).length,
    unknownBalanceStations: states.filter((state) => state === "untrusted" || state === "missing").length,
    staleBalanceStations: states.filter((state) => state === "stale").length,
    primaryBalanceCurrency: BALANCE_CURRENCY,
    stationUsage: summarizeStationUsage(currentBalances, creditPerCnyByStation),
  };
}

function summarizeStationUsage(
  snapshots: BalanceSnapshot[],
  creditPerCnyByStation: Map<string, number>,
): DashboardStationUsageSummary {
  return {
    todayRequestCount: sumNumbers(snapshots.map((snapshot) => snapshot.todayRequestCount)),
    totalRequestCount: sumNumbers(snapshots.map((snapshot) => snapshot.totalRequestCount)),
    todayConsumption: sumConsumption(snapshots, "todayConsumption", creditPerCnyByStation),
    totalConsumption: sumConsumption(snapshots, "totalConsumption", creditPerCnyByStation),
    todayBaseConsumption: sumBaseConsumption(
      snapshots,
      "todayBaseConsumption",
      creditPerCnyByStation,
    ),
    totalBaseConsumption: sumBaseConsumption(
      snapshots,
      "totalBaseConsumption",
      creditPerCnyByStation,
    ),
    todayTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.todayTokenCount)),
    totalTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.totalTokenCount)),
    todayInputTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.todayInputTokenCount)),
    todayOutputTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.todayOutputTokenCount)),
    totalInputTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.totalInputTokenCount)),
    totalOutputTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.totalOutputTokenCount)),
  };
}

function sumConsumption(
  snapshots: BalanceSnapshot[],
  field: "todayConsumption" | "totalConsumption",
  creditPerCnyByStation: Map<string, number>,
) {
  return snapshots.reduce<number>((sum, snapshot) => {
    const value = snapshot[field];
    if (typeof value !== "number" || !Number.isFinite(value)) {
      return sum;
    }
    return sum + value / (creditPerCnyByStation.get(snapshot.stationId) ?? 1);
  }, 0);
}

function sumBaseConsumption(
  snapshots: BalanceSnapshot[],
  baseField: "todayBaseConsumption" | "totalBaseConsumption",
  creditPerCnyByStation: Map<string, number>,
) {
  let hasValue = false;
  const total = snapshots.reduce<number>((sum, snapshot) => {
    const value = snapshot[baseField];
    if (typeof value !== "number" || !Number.isFinite(value)) {
      return sum;
    }
    hasValue = true;
    return sum + value / (creditPerCnyByStation.get(snapshot.stationId) ?? 1);
  }, 0);
  return hasValue ? total : null;
}

function sumNumbers(values: Array<number | null | undefined>): number {
  return values.reduce<number>(
    (sum, value) => sum + (typeof value === "number" && Number.isFinite(value) ? value : 0),
    0,
  );
}

function safeCreditPerCny(value: number) {
  return Number.isFinite(value) && value > 0 ? value : 1;
}
