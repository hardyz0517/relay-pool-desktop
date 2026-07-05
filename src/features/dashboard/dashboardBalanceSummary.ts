import type { BalanceSnapshot } from "@/lib/types/economics";

export type DashboardBalanceSummary = {
  latestStationBalances: BalanceSnapshot[];
  totalBalance: number;
  lowBalanceStations: number;
  primaryBalanceCurrency: string | undefined;
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
  };
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}

