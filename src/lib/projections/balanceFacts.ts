import { toTimestampMillis } from "@/lib/time";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";

export type StationBalanceCurrentFact = {
  stationId: string;
  snapshotId: string | null;
  value: number | null;
  currency: string;
  lowBalanceThreshold: number | null;
  status: string | null;
  source: "balance_snapshot" | "station_cache" | "missing";
  sourceLabel: string;
  updatedAt: string | null;
  collectedAt: string | null;
  sourceSnapshot: BalanceSnapshot | null;
};

export function buildCurrentStationBalanceFacts(input: {
  stations: Station[];
  balances: BalanceSnapshot[];
}): Map<string, StationBalanceCurrentFact> {
  const latestStationBalances = latestStationBalanceSnapshotsByStation(input.balances);
  return new Map(
    input.stations.map((station) => [
      station.id,
      factForStation(station, latestStationBalances.get(station.id) ?? null),
    ]),
  );
}

export function currentStationBalanceFor(input: {
  station: Station;
  balances: BalanceSnapshot[];
}): StationBalanceCurrentFact {
  return factForStation(
    input.station,
    latestStationBalanceSnapshotsByStation(input.balances).get(input.station.id) ?? null,
  );
}

export function latestStationBalanceSnapshots(balances: BalanceSnapshot[]) {
  return Array.from(latestStationBalanceSnapshotsByStation(balances).values());
}

function latestStationBalanceSnapshotsByStation(balances: BalanceSnapshot[]) {
  const latest = new Map<string, BalanceSnapshot>();
  for (const balance of balances) {
    if (balance.scope !== "station") {
      continue;
    }
    const current = latest.get(balance.stationId);
    if (!current || isNewerBalanceSnapshot(balance, current)) {
      latest.set(balance.stationId, balance);
    }
  }
  return latest;
}

function isNewerBalanceSnapshot(candidate: BalanceSnapshot, current: BalanceSnapshot) {
  const updatedAtDifference = toTime(candidate.updatedAt) - toTime(current.updatedAt);
  if (updatedAtDifference !== 0) {
    return updatedAtDifference > 0;
  }

  const createdAtDifference = toTime(candidate.createdAt) - toTime(current.createdAt);
  if (createdAtDifference !== 0) {
    return createdAtDifference > 0;
  }

  return candidate.id > current.id;
}

function factForStation(
  station: Station,
  snapshot: BalanceSnapshot | null,
): StationBalanceCurrentFact {
  if (snapshot) {
    return {
      stationId: station.id,
      snapshotId: snapshot.id,
      value: snapshot.value,
      currency: snapshot.currency || "CNY",
      lowBalanceThreshold: snapshot.lowBalanceThreshold,
      status: snapshot.status,
      source: "balance_snapshot",
      sourceLabel: snapshot.source,
      updatedAt: snapshot.updatedAt,
      collectedAt: snapshot.collectedAt,
      sourceSnapshot: snapshot,
    };
  }

  if (
    typeof station.balanceCny === "number" ||
    typeof station.lowBalanceThresholdCny === "number" ||
    station.lastCheckedAt
  ) {
    return {
      stationId: station.id,
      snapshotId: null,
      value: finiteOrNull(station.balanceCny),
      currency: "CNY",
      lowBalanceThreshold: finiteOrNull(station.lowBalanceThresholdCny),
      status: balanceStatusFor(station.balanceCny, station.lowBalanceThresholdCny),
      source: "station_cache",
      sourceLabel: "station_config",
      updatedAt: station.lastCheckedAt,
      collectedAt: null,
      sourceSnapshot: null,
    };
  }

  return {
    stationId: station.id,
    snapshotId: null,
    value: null,
    currency: "CNY",
    lowBalanceThreshold: null,
    status: null,
    source: "missing",
    sourceLabel: "missing",
    updatedAt: null,
    collectedAt: null,
    sourceSnapshot: null,
  };
}

function balanceStatusFor(value: number | null, threshold: number | null) {
  if (value == null || !Number.isFinite(value)) {
    return null;
  }
  if (value <= 0) {
    return "depleted";
  }
  if (threshold != null && Number.isFinite(threshold) && value <= threshold) {
    return "low";
  }
  return "normal";
}

function finiteOrNull(value: number | null) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const time = toTimestampMillis(value);
  return Number.isNaN(time) ? 0 : time;
}
