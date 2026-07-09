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

export type StationIssueTag = {
  label: string;
  tone: "info" | "warning" | "error" | "disabled";
  title?: string;
};

export type StationAssetRow = {
  station: Station;
  enabledKeyCount: number;
  warningKeyCount: number;
  groupIssueCount: number;
  missingRateCount: number;
  balanceFactsReady: boolean;
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
  balanceFactsReady = true,
}: {
  stations: Station[];
  keysByStation: Map<string, StationKey[]>;
  balances: BalanceSnapshot[];
  snapshotsByStation: Map<string, CollectorSnapshot | null>;
  groupBindingsByStation: Map<string, StationGroupBinding[]>;
  groupRatesByStation?: Map<string, GroupRateRecord[]>;
  changes: ChangeEvent[];
  balanceFactsReady?: boolean;
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
      groupIssueCount: countGroupIssues(groupBindings),
      missingRateCount: countMissingRates(groupBindings, groupRates),
      balanceFactsReady,
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

export function stationIssueTags(row: StationAssetRow): StationIssueTag[] {
  const tags: StationIssueTag[] = [];
  const balanceValue = row.currentBalance.value;
  const lowBalanceThreshold = row.currentBalance.lowBalanceThreshold ?? row.station.lowBalanceThresholdCny;
  const collectionTag = stationCollectionIssueTag(row);

  if (!row.station.enabled || row.station.status === "disabled") {
    tags.push({ label: "已禁用", tone: "disabled" });
  }

  if (!row.station.apiKeyPresent && row.station.keyCount === 0) {
    tags.push({ label: "缺 API Key", tone: "error" });
  } else if (row.station.keyCount > 0 && row.enabledKeyCount === 0) {
    tags.push({ label: "无可用 Key", tone: "warning" });
  }

  if (row.warningKeyCount > 0) {
    tags.push({ label: "Key 异常", tone: "warning" });
  }

  if (collectionTag) {
    tags.push(collectionTag);
  }

  if (balanceValue == null && row.balanceFactsReady) {
    tags.push({ label: "余额未采集", tone: "info" });
  } else if (typeof balanceValue === "number" && balanceValue <= 0) {
    tags.push({ label: "余额为零", tone: "error" });
  } else if (
    row.currentBalance.status === "low" ||
    (typeof balanceValue === "number" && typeof lowBalanceThreshold === "number" && balanceValue < lowBalanceThreshold)
  ) {
    tags.push({ label: "余额偏低", tone: "warning" });
  }

  if (row.groupIssueCount > 0) {
    tags.push({ label: "分组异常", tone: "warning" });
  }

  if (row.missingRateCount > 0) {
    tags.push({ label: "倍率缺失", tone: "warning" });
  }

  return tags;
}

function stationCollectionIssueTag(row: StationAssetRow): StationIssueTag | null {
  const snapshotStatus = row.latestSnapshot?.status;
  const snapshotSummary = row.latestSnapshot?.summaryJson ?? {};
  const loginRequired =
    row.latestSnapshot?.status === "manual_required" ||
    snapshotSummary.loginRequired === true ||
    snapshotSummary.loginStatus === "manual_required";

  if (loginRequired) {
    return { label: "需登录", tone: "warning", title: row.latestSnapshot?.errorMessage ?? "采集需要登录或人工处理" };
  }

  if (snapshotStatus === "failed" || snapshotStatus === "error" || row.station.status === "error") {
    return { label: "采集失败", tone: "error", title: row.latestSnapshot?.errorMessage ?? "最近一次采集失败" };
  }

  if (row.station.status === "unchecked" && !row.latestSnapshot) {
    return { label: "未采集", tone: "info" };
  }

  return null;
}

function countGroupIssues(bindings: StationGroupBinding[]) {
  return bindings.filter(
    (binding) =>
      binding.bindingKind === "station_group" &&
      (binding.bindingStatus === "missing" || binding.bindingStatus === "disabled"),
  ).length;
}

function countMissingRates(bindings: StationGroupBinding[], rates: GroupRateRecord[]) {
  const collectedBindingsWithoutRates = bindings.filter(
    (binding) =>
      binding.bindingKind === "station_group" &&
      binding.bindingStatus !== "missing" &&
      binding.bindingStatus !== "disabled" &&
      binding.bindingStatus !== "manual_legacy" &&
      binding.effectiveRateMultiplier == null,
  ).length;
  const missingRateRecords = rates.filter((rate) => rate.effectiveRateMultiplier == null).length;
  return collectedBindingsWithoutRates + missingRateRecords;
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

