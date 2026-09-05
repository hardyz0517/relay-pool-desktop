import { BALANCE_CURRENCY } from "@/lib/balanceCurrency";
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
  /** Current balance facts are either a typed snapshot or explicitly absent.
   * Station compatibility cache columns are intentionally not an authority. */
  source: "balance_snapshot" | "missing";
  sourceLabel: string;
  updatedAt: string | null;
  collectedAt: string | null;
  sourceSnapshot: BalanceSnapshot | null;
  state: "available" | "depleted" | "stale" | "untrusted" | "missing";
  eligibleForRouting: boolean;
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

export function balanceSnapshotState(
  snapshot: BalanceSnapshot,
  evaluationAtMs = Date.now(),
): StationBalanceCurrentFact["state"] {
  const validScopeAndKind =
    snapshot.stationKeyId === null &&
    (snapshot.scope === "station" || snapshot.scope === "station_account") &&
    snapshot.balanceKind === "account_balance";
  if (!validScopeAndKind || !snapshot.currency.trim()) {
    return "untrusted";
  }
  if (
    snapshot.evidenceConfidence !== "confirmed" ||
    snapshot.spendabilityAuthority !== "authoritative"
  ) {
    return "untrusted";
  }
  if (snapshot.validUntilMs !== null && snapshot.validUntilMs < evaluationAtMs) {
    return "stale";
  }
  if (snapshot.value === null) {
    return /^(depleted|exhausted|empty)$/i.test(snapshot.status.trim())
      ? "depleted"
      : "missing";
  }
  if (typeof snapshot.value !== "number" || !Number.isFinite(snapshot.value)) {
    return "missing";
  }
  if (
    snapshot.value <= 0
  ) {
    return "depleted";
  }
  return "available";
}

export function isBalanceSnapshotEligibleForRouting(
  snapshot: BalanceSnapshot,
  evaluationAtMs = Date.now(),
) {
  return balanceSnapshotState(snapshot, evaluationAtMs) === "available";
}

export function latestStationBalanceSnapshotsByStation(balances: BalanceSnapshot[]) {
  const latest = new Map<string, BalanceSnapshot>();
  for (const balance of balances) {
    if (
      !["station", "station_account"].includes(balance.scope) ||
      balance.stationKeyId !== null ||
      balance.balanceKind !== "account_balance"
    ) {
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
    const state = balanceSnapshotState(snapshot);
    return {
      stationId: station.id,
      snapshotId: snapshot.id,
      value: snapshot.value,
      currency: BALANCE_CURRENCY,
      lowBalanceThreshold: snapshot.lowBalanceThreshold,
      status: snapshot.status,
      source: "balance_snapshot",
      sourceLabel: snapshot.source,
      updatedAt: snapshot.updatedAt,
      collectedAt: snapshot.collectedAt,
      sourceSnapshot: snapshot,
      state,
      eligibleForRouting: state === "available",
    };
  }

  return {
    stationId: station.id,
    snapshotId: null,
    value: null,
    currency: BALANCE_CURRENCY,
    lowBalanceThreshold: null,
    status: null,
    source: "missing",
    sourceLabel: "missing",
    updatedAt: null,
    collectedAt: null,
    sourceSnapshot: null,
    state: "missing",
    eligibleForRouting: false,
  };
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const time = toTimestampMillis(value);
  return Number.isNaN(time) ? 0 : time;
}
