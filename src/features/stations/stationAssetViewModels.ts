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

export type StationIssueTagKind =
  | "disabled"
  | "missing_api_key"
  | "no_enabled_key"
  | "key_warning"
  | "login_required"
  | "collection_failed"
  | "not_collected"
  | "balance_missing"
  | "balance_zero"
  | "balance_low"
  | "group_issue"
  | "missing_rate";

export type StationIssueTag = {
  kind: StationIssueTagKind;
  label: string;
  tone: "info" | "warning" | "error" | "disabled";
  title?: string;
};

export type StationGroupIssueReason = {
  bindingId: string;
  groupName: string;
  bindingStatus: "missing" | "disabled";
  affectedKeyCount: number;
  affectedKeyNames: string[];
  message: string;
};

export type StationIssueFilterValue = "all" | StationIssueTagKind;

type StationIssueTagDefinition = Omit<StationIssueTag, "kind" | "title">;

const STATION_ISSUE_TAG_DEFINITIONS: Record<StationIssueTagKind, StationIssueTagDefinition> = {
  disabled: { label: "已禁用", tone: "disabled" },
  missing_api_key: { label: "缺 API Key", tone: "error" },
  no_enabled_key: { label: "无可用 Key", tone: "warning" },
  key_warning: { label: "Key 异常", tone: "warning" },
  login_required: { label: "需登录", tone: "warning" },
  collection_failed: { label: "采集失败", tone: "error" },
  not_collected: { label: "未采集", tone: "info" },
  balance_missing: { label: "余额未采集", tone: "info" },
  balance_zero: { label: "余额为零", tone: "error" },
  balance_low: { label: "余额偏低", tone: "warning" },
  group_issue: { label: "分组异常", tone: "warning" },
  missing_rate: { label: "倍率缺失", tone: "warning" },
};

export const STATION_ISSUE_FILTER_OPTIONS: Array<{ value: StationIssueFilterValue; label: string }> = [
  { value: "all", label: "全部问题" },
  { value: "balance_zero", label: STATION_ISSUE_TAG_DEFINITIONS.balance_zero.label },
  { value: "balance_low", label: STATION_ISSUE_TAG_DEFINITIONS.balance_low.label },
  { value: "balance_missing", label: STATION_ISSUE_TAG_DEFINITIONS.balance_missing.label },
  { value: "login_required", label: STATION_ISSUE_TAG_DEFINITIONS.login_required.label },
  { value: "collection_failed", label: STATION_ISSUE_TAG_DEFINITIONS.collection_failed.label },
  { value: "missing_api_key", label: STATION_ISSUE_TAG_DEFINITIONS.missing_api_key.label },
  { value: "no_enabled_key", label: STATION_ISSUE_TAG_DEFINITIONS.no_enabled_key.label },
  { value: "key_warning", label: STATION_ISSUE_TAG_DEFINITIONS.key_warning.label },
  { value: "group_issue", label: STATION_ISSUE_TAG_DEFINITIONS.group_issue.label },
  { value: "missing_rate", label: STATION_ISSUE_TAG_DEFINITIONS.missing_rate.label },
  { value: "disabled", label: STATION_ISSUE_TAG_DEFINITIONS.disabled.label },
  { value: "not_collected", label: STATION_ISSUE_TAG_DEFINITIONS.not_collected.label },
];

export type StationAssetRow = {
  station: Station;
  enabledKeyCount: number;
  warningKeyCount: number;
  groupIssueCount: number;
  groupIssueReasons: StationGroupIssueReason[];
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
    const groupIssueReasons = buildStationGroupIssueReasons(groupBindings, keys);
    return {
      station,
      enabledKeyCount,
      warningKeyCount: keys.filter((key) => key.status === "warning" || key.status === "error").length,
      groupIssueCount: groupIssueReasons.length,
      groupIssueReasons,
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
    tags.push(createStationIssueTag("disabled"));
  }

  if (!row.station.apiKeyPresent && row.station.keyCount === 0) {
    tags.push(createStationIssueTag("missing_api_key"));
  } else if (row.station.keyCount > 0 && row.enabledKeyCount === 0) {
    tags.push(createStationIssueTag("no_enabled_key"));
  }

  if (row.warningKeyCount > 0) {
    tags.push(createStationIssueTag("key_warning"));
  }

  if (collectionTag) {
    tags.push(collectionTag);
  }

  if (balanceValue == null && row.balanceFactsReady) {
    tags.push(createStationIssueTag("balance_missing"));
  } else if (typeof balanceValue === "number" && balanceValue <= 0) {
    tags.push(createStationIssueTag("balance_zero"));
  } else if (
    row.currentBalance.status === "low" ||
    (typeof balanceValue === "number" && typeof lowBalanceThreshold === "number" && balanceValue < lowBalanceThreshold)
  ) {
    tags.push(createStationIssueTag("balance_low"));
  }

  if (row.groupIssueCount > 0) {
    const title = row.groupIssueReasons?.map((reason) => reason.message).join("\n");
    tags.push(createStationIssueTag("group_issue", title));
  }

  if (row.missingRateCount > 0) {
    tags.push(createStationIssueTag("missing_rate"));
  }

  return tags;
}

export function filterStationAssetRowsByIssue(
  rows: StationAssetRow[],
  issueFilter: StationIssueFilterValue,
): StationAssetRow[] {
  if (issueFilter === "all") {
    return rows;
  }
  return rows.filter((row) => stationIssueTags(row).some((tag) => tag.kind === issueFilter));
}

function stationCollectionIssueTag(row: StationAssetRow): StationIssueTag | null {
  const snapshotStatus = row.latestSnapshot?.status;
  const snapshotSummary = row.latestSnapshot?.summaryJson ?? {};
  const loginRequired =
    row.latestSnapshot?.status === "manual_required" ||
    snapshotSummary.loginRequired === true ||
    snapshotSummary.loginStatus === "manual_required";

  if (loginRequired) {
    return createStationIssueTag("login_required", row.latestSnapshot?.errorMessage ?? "采集需要登录或人工处理");
  }

  if (snapshotStatus === "failed" || snapshotStatus === "error" || row.station.status === "error") {
    return createStationIssueTag("collection_failed", row.latestSnapshot?.errorMessage ?? "最近一次采集失败");
  }

  if (row.station.status === "unchecked" && !row.latestSnapshot) {
    return createStationIssueTag("not_collected");
  }

  return null;
}

function createStationIssueTag(kind: StationIssueTagKind, title?: string): StationIssueTag {
  return {
    kind,
    ...STATION_ISSUE_TAG_DEFINITIONS[kind],
    ...(title ? { title } : {}),
  };
}

function buildStationGroupIssueReasons(
  bindings: StationGroupBinding[],
  keys: StationKey[],
): StationGroupIssueReason[] {
  const currentGroupNames = new Set(
    bindings
      .filter(
        (binding) =>
          binding.bindingKind === "station_group" && binding.bindingStatus === "available",
      )
      .map((binding) => normalizeGroupName(binding.groupName))
      .filter(Boolean),
  );
  const historicalGroups = new Map<
    string,
    {
      bindingId: string;
      groupName: string;
      bindingStatus: "missing" | "disabled";
      bindingIds: Set<string>;
      groupIdHashes: Set<string>;
    }
  >();

  for (const binding of bindings) {
    if (
      binding.bindingKind !== "station_group" ||
      (binding.bindingStatus !== "missing" && binding.bindingStatus !== "disabled")
    ) {
      continue;
    }
    const normalizedName = normalizeGroupName(binding.groupName);
    if (!normalizedName || currentGroupNames.has(normalizedName)) {
      continue;
    }
    const identity = `${binding.bindingStatus}:${normalizedName}`;
    const group = historicalGroups.get(identity) ?? {
      bindingId: binding.id,
      groupName: binding.groupName.trim(),
      bindingStatus: binding.bindingStatus,
      bindingIds: new Set<string>(),
      groupIdHashes: new Set<string>(),
    };
    group.bindingIds.add(binding.id);
    if (binding.groupIdHash?.trim()) {
      group.groupIdHashes.add(binding.groupIdHash.trim());
    }
    historicalGroups.set(identity, group);
  }

  const enabledKeys = keys.filter((key) => key.enabled);
  return [...historicalGroups.entries()].flatMap(([identity, group]) => {
    const normalizedName = identity.slice(identity.indexOf(":") + 1);
    const affectedKeys = enabledKeys.filter((key) => {
      if (key.groupBindingId?.trim()) {
        return group.bindingIds.has(key.groupBindingId.trim());
      }
      if (key.groupIdHash?.trim()) {
        return group.groupIdHashes.has(key.groupIdHash.trim());
      }
      return Boolean(key.groupName?.trim() && normalizeGroupName(key.groupName) === normalizedName);
    });
    if (affectedKeys.length === 0) {
      return [];
    }

    const affectedKeyNames = [...new Set(affectedKeys.map((key) => key.name.trim()).filter(Boolean))];
    return [{
      bindingId: group.bindingId,
      groupName: group.groupName,
      bindingStatus: group.bindingStatus,
      affectedKeyCount: affectedKeys.length,
      affectedKeyNames,
      message: stationGroupIssueMessage(
        group.groupName,
        group.bindingStatus,
        affectedKeys.length,
        affectedKeyNames,
      ),
    }];
  });
}

function normalizeGroupName(value: string) {
  return value.trim().toLocaleLowerCase().replace(/\s+/g, " ");
}

function stationGroupIssueMessage(
  groupName: string,
  bindingStatus: "missing" | "disabled",
  affectedKeyCount: number,
  affectedKeyNames: string[],
) {
  const statusText = bindingStatus === "missing" ? "已下架" : "已禁用";
  const visibleNames = affectedKeyNames.slice(0, 3);
  if (affectedKeyCount === 1 && visibleNames.length === 1) {
    return `分组「${groupName}」${statusText}，但仍被启用 Key「${visibleNames[0]}」使用。`;
  }
  const nameSuffix = visibleNames.length > 0
    ? `：${visibleNames.join("、")}${affectedKeyNames.length > visibleNames.length ? "等" : ""}`
    : "";
  return `分组「${groupName}」${statusText}，但仍被 ${affectedKeyCount} 个启用 Key 使用${nameSuffix}。`;
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

