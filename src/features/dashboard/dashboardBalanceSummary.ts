import type { BalanceSnapshot } from "@/lib/types/economics";

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
};

export function summarizeDashboardBalances(balances: BalanceSnapshot[]): DashboardBalanceSummary {
  const latestByStation = new Map<string, BalanceSnapshot>();

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
    stationUsage: summarizeStationUsage(latestStationBalances),
  };
}

function summarizeStationUsage(snapshots: BalanceSnapshot[]): DashboardStationUsageSummary {
  return {
    todayRequestCount: sumNumbers(snapshots.map((snapshot) => snapshot.todayRequestCount)),
    totalRequestCount: sumNumbers(snapshots.map((snapshot) => snapshot.totalRequestCount)),
    todayConsumption: sumNumbers(snapshots.map((snapshot) => snapshot.todayConsumption)),
    totalConsumption: sumNumbers(snapshots.map((snapshot) => snapshot.totalConsumption)),
    todayTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.todayTokenCount)),
    totalTokenCount: sumNumbers(snapshots.map((snapshot) => snapshot.totalTokenCount)),
  };
}

function sumNumbers(values: Array<number | null | undefined>): number {
  return values.reduce<number>(
    (sum, value) => sum + (typeof value === "number" && Number.isFinite(value) ? value : 0),
    0,
  );
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}
