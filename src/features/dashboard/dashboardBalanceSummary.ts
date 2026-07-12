import type { BalanceSnapshot } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";

export type DashboardBalanceSummary = {
  latestStationBalances: BalanceSnapshot[];
  totalBalance: number;
  lowBalanceStations: number;
  primaryBalanceCurrency: string | undefined;
  stationUsage: DashboardStationUsageSummary;
};

export type DashboardStationUsageSummary = {
  todayRequestCount: number;
  totalRequestCount: number;
  todayConsumption: number;
  totalConsumption: number;
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
  const latestByStation = new Map<string, BalanceSnapshot>();
  const creditPerCnyByStation = new Map(
    stations.map((station) => [station.id, safeCreditPerCny(station.creditPerCny)]),
  );

  for (const balance of balances) {
    if (balance.scope !== "station") {
      continue;
    }
    const current = latestByStation.get(balance.stationId);
    if (!current || toTime(balance.updatedAt) > toTime(current.updatedAt)) {
      latestByStation.set(balance.stationId, balance);
    }
  }

  const latestStationBalances = Array.from(latestByStation.values());
  return {
    latestStationBalances,
    totalBalance: latestStationBalances.reduce((sum, snapshot) => sum + (snapshot.value ?? 0), 0),
    lowBalanceStations: latestStationBalances.filter(
      (snapshot) => snapshot.status === "low" || snapshot.status === "depleted",
    ).length,
    primaryBalanceCurrency: latestStationBalances.find((snapshot) => snapshot.currency)?.currency,
    stationUsage: summarizeStationUsage(latestStationBalances, creditPerCnyByStation),
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

function sumNumbers(values: Array<number | null | undefined>): number {
  return values.reduce<number>(
    (sum, value) => sum + (typeof value === "number" && Number.isFinite(value) ? value : 0),
    0,
  );
}

function safeCreditPerCny(value: number) {
  return Number.isFinite(value) && value > 0 ? value : 1;
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}
