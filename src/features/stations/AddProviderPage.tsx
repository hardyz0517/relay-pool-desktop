import { useEffect, useMemo, useState, type FormEvent } from "react";
import { ArrowLeft, Check, KeyRound, LogIn, Plus, RefreshCw, ShieldCheck } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, ConfirmDialog, IconButton, PageForm, SectionCard, SelectControl, useToast } from "@/components/ui";
import { collectStationTask, startManualAuthorization, testStationLoginInput } from "@/lib/api/collector";
import { listGroupRateRecords, listStationGroupBindings, upsertStationGroupBinding } from "@/lib/api/groupFacts";
import {
  bindRemoteStationKey,
  createLocalStationKeyFromRemote,
  createStationKey,
  createRemoteStationKey,
  deleteStationKey,
  getStationCredentials,
  getRemoteKeyCapability,
  listStationKeys,
  listRemoteStationKeys,
  scanRemoteStationKeys,
  unbindRemoteStationKey,
  updateStationCredentials,
  updateStationKey,
} from "@/lib/api/stationKeys";
import { createStation, listStations, updateStation } from "@/lib/api/stations";
import { getSettings } from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import { inferGroupCategoryFromEvidence, normalizeGroupCategory } from "@/lib/groupCategories";
import { effectiveRateMultiplierForCredit } from "@/lib/formatters";
import { DEFAULT_MANUAL_PROXY_URL, withManualProxyDefault } from "@/lib/proxyDefaults";
import { buildCurrentStationGroupFacts } from "@/lib/projections/groupFacts";
import {
  isCollectedStationGroupBinding,
  type GroupRateRecord,
  type StationGroupBinding,
  type StationGroupOption,
  type UpsertStationGroupBindingInput,
} from "@/lib/types/groupFacts";
import type { RemoteKeyCapability, RemoteStationKey, StationCredentials, StationKey } from "@/lib/types/stationKeys";
import {
  stationProxyModeLabels,
  stationTypeOptions,
  type Station,
  type StationProxyMode,
  type StationType,
} from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import {
  createEmptyStationKeyDraft,
  StationKeyRowsEditor,
  type StationKeyDraft,
  type StationKeyGroupOption,
} from "./components/StationKeyRowsEditor";
import {
  createEmptyStationGroupDraft,
  StationGroupRowsEditor,
  type StationGroupDraft,
} from "./components/StationGroupRowsEditor";
import { CreateRemoteKeyDialog } from "./components/CreateRemoteKeyDialog";
import { RemoteKeyDiscoveryList } from "./components/RemoteKeyDiscoveryList";
import {
  buildStationGroupOptionFromRawMultiplierForSelect,
  buildStationGroupOptionsFromCurrentFactsForSelect,
  findMatchingGroupOption,
  formatMultiplier,
  normalizeStationGroupOptions,
} from "./groupOptionViewModels";
import { providerPresets, type ProviderPresetId } from "./providerPresets";

type AddProviderPageProps = {
  stationId?: string | null;
  onBack: () => void;
  onCreated?: () => void;
  onUpdated?: () => void;
};

type AddProviderFormState = {
  presetId: ProviderPresetId;
  name: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
  apiKey: string;
  collectorProxyMode: StationProxyMode;
  collectorProxyUrl: string;
  enabled: boolean;
  creditPerCny: string;
  loginUsername: string;
  loginPassword: string;
  rememberPassword: boolean;
  lowBalanceThresholdCny: string;
  collectionIntervalMinutes: string;
  note: string;
};

type ConnectionTestState = {
  status: "idle" | "testing" | "success" | "warning" | "error";
  message: string | null;
};

type RemoteCreateInput = {
  name: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupName: string | null;
};

const defaultPreset = providerPresets[0];

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";
const remoteLocalKeyNotePrefix = "由远端发现开关自动创建";

function createDefaultProviderForm(): AddProviderFormState {
  return {
    presetId: defaultPreset.id,
    name: getPresetDefaultStationName(defaultPreset),
    stationType: defaultPreset.stationType,
    websiteUrl: defaultPreset.websiteUrl,
    apiBaseUrl: defaultPreset.apiBaseUrl,
    apiKey: "",
    collectorProxyMode: "inherit",
    collectorProxyUrl: "",
    enabled: true,
    creditPerCny: "1",
    loginUsername: "",
    loginPassword: "",
    rememberPassword: false,
    lowBalanceThresholdCny: "",
    collectionIntervalMinutes: "5",
    note: "",
  };
}

function serializeProviderDraft(
  form: AddProviderFormState,
  groupRows: StationGroupDraft[],
  keyRows: StationKeyDraft[],
) {
  return JSON.stringify({
    form,
    groupRows: normalizeProviderGroupRowsForDirtyCheck(groupRows),
    keyRows: normalizeProviderKeyRowsForDirtyCheck(keyRows),
  });
}

function normalizeProviderGroupRowsForDirtyCheck(rows: StationGroupDraft[]) {
  return rows.map((row) => ({
    groupBindingId: row.groupBindingId,
    groupKeyHash: row.groupKeyHash,
    groupIdHash: row.groupIdHash,
    groupName: row.groupName,
    rateMultiplier: row.rateMultiplier,
    inferredGroupCategory: row.inferredGroupCategory,
    groupCategoryOverride: row.groupCategoryOverride,
    source: row.source,
    deleteRequested: row.deleteRequested,
  }));
}

function normalizeProviderKeyRowsForDirtyCheck(rows: StationKeyDraft[]) {
  return rows.map((row) => ({
    id: row.id,
    name: row.name,
    apiKey: row.apiKey,
    groupBindingId: row.groupBindingId,
    groupIdHash: row.groupIdHash,
    groupName: row.groupName,
    rateMultiplier: row.rateMultiplier,
    enabled: row.enabled,
    note: row.note,
    deleteRequested: row.deleteRequested,
  }));
}

function getPresetDefaultStationName(preset: (typeof providerPresets)[number]) {
  return preset.id === "custom" ? "" : preset.name;
}

function draftRemoteCapability(stationType: StationType): RemoteKeyCapability {
  if (stationType === "sub2api" || stationType === "newapi") {
    return {
      stationId: "",
      stationType,
      canListRemoteKeys: true,
      canCreateRemoteKey: true,
      canReadGroups: true,
      requiresManualSession: true,
      unsupportedReason: null,
    };
  }
  return {
    stationId: "",
    stationType,
    canListRemoteKeys: false,
    canCreateRemoteKey: false,
    canReadGroups: false,
    requiresManualSession: false,
    unsupportedReason: `当前仅 Sub2API 支持获取远端 Key`,
  };
}

function formFromStation(station: Station, credentials: StationCredentials): AddProviderFormState {
  const preset = providerPresets.find((item) => item.stationType === station.stationType) ?? defaultPreset;
  return {
    presetId: preset.id,
    name: station.name,
    stationType: station.stationType,
    websiteUrl: station.websiteUrl,
    apiBaseUrl: station.apiBaseUrl,
    apiKey: "",
    collectorProxyMode: station.collectorProxyMode,
    collectorProxyUrl: station.collectorProxyUrl ?? "",
    enabled: station.enabled,
    creditPerCny: String(station.creditPerCny),
    loginUsername: credentials.loginUsername ?? "",
    loginPassword: "",
    rememberPassword: credentials.rememberPassword,
    lowBalanceThresholdCny:
      station.lowBalanceThresholdCny === null ? "" : String(station.lowBalanceThresholdCny),
    collectionIntervalMinutes: String(station.collectionIntervalMinutes),
    note: station.note ?? "",
  };
}

function keyToDraft(key: StationKey): StationKeyDraft {
  return {
    clientId: key.id,
    id: key.id,
    name: key.name,
    apiKey: "",
    groupBindingId: key.groupBindingId,
    groupIdHash: key.groupIdHash,
    groupName: key.groupName ?? "",
    rateMultiplier: key.rateMultiplier === null ? "" : String(key.rateMultiplier),
    enabled: key.enabled,
    note: key.note ?? "",
    deleteRequested: false,
  };
}

function groupBindingsToDrafts(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
): StationGroupDraft[] {
  const latestRates = latestStationGroupRatesByBindingId(rates);
  return bindings
    .filter(isCollectedStationGroupBinding)
    .map((binding) => groupBindingToDraft(binding, latestRates.get(binding.id) ?? null));
}

function latestStationGroupRatesByBindingId(rates: GroupRateRecord[]) {
  const latestRates = new Map<string, GroupRateRecord>();
  rates.forEach((rate) => {
    if (rate.bindingKind !== "station_group" || !rate.groupBindingId) {
      return;
    }
    const current = latestRates.get(rate.groupBindingId);
    if (!current || Date.parse(rate.checkedAt) > Date.parse(current.checkedAt)) {
      latestRates.set(rate.groupBindingId, rate);
    }
  });
  return latestRates;
}

function groupBindingToDraft(
  binding: StationGroupBinding,
  latestRate: GroupRateRecord | null = null,
): StationGroupDraft {
  const rateMultiplier =
    binding.userRateMultiplier ??
    binding.effectiveRateMultiplier ??
    binding.defaultRateMultiplier ??
    latestRate?.userRateMultiplier ??
    latestRate?.effectiveRateMultiplier ??
    latestRate?.defaultRateMultiplier;
  const inferredGroupCategory =
    normalizeGroupCategory(binding.inferredGroupCategory) ??
    normalizeGroupCategory(latestRate?.inferredGroupCategory) ??
    inferGroupCategoryFromEvidence({
      groupName: binding.groupName || latestRate?.groupName || "",
      rawJsonRedacted: latestRate?.rawJsonRedacted ?? binding.rawJsonRedacted,
    });
  return {
    clientId: binding.id,
    groupBindingId: binding.id,
    groupKeyHash: binding.groupKeyHash,
    groupIdHash: binding.groupIdHash,
    groupName: binding.groupName || latestRate?.groupName || "",
    rateMultiplier: rateMultiplier == null ? "" : String(rateMultiplier),
    inferredGroupCategory,
    groupCategoryOverride: normalizeGroupCategory(binding.groupCategoryOverride),
    source: isRemoteGroupSource(binding.rateSource ?? latestRate?.source ?? null) ? "remote" : "manual",
    deleteRequested: false,
  };
}

function isRemoteGroupSource(source: string | null) {
  if (!source) {
    return false;
  }
  return source !== "manual" && source !== "manual_legacy" && source !== "legacy_key_group";
}

function rowHasMeaningfulContent(row: StationKeyDraft) {
  return Boolean(
      row.id ||
      row.name.trim() ||
      row.apiKey.trim() ||
      row.groupBindingId ||
      row.groupIdHash ||
      row.groupName.trim() ||
      row.rateMultiplier.trim() ||
      row.note.trim(),
  );
}

function groupRowHasMeaningfulContent(row: StationGroupDraft) {
  return Boolean(
    row.groupBindingId ||
      row.groupKeyHash.trim() ||
      row.groupIdHash ||
      row.groupName.trim() ||
      row.rateMultiplier.trim(),
  );
}

function groupDraftToOption(row: StationGroupDraft, creditPerCny = 1): StationKeyGroupOption | null {
  if (row.deleteRequested || !row.groupName.trim()) {
    return null;
  }
  return {
    value: row.groupBindingId
      ? `binding:${row.groupBindingId}`
      : row.groupIdHash
        ? `remote:${row.groupIdHash}`
        : `name:${row.groupName.trim()}`,
    groupBindingId: row.groupBindingId,
    groupIdHash: row.groupIdHash,
    groupName: row.groupName.trim(),
    rateMultiplier: effectiveRateMultiplierForCredit(parseDraftRateMultiplier(row.rateMultiplier), creditPerCny),
    inferredGroupCategory: row.inferredGroupCategory,
    groupCategoryOverride: row.groupCategoryOverride,
    effectiveGroupCategory: row.groupCategoryOverride ?? row.inferredGroupCategory,
    rateSource: null,
    selectableForRemoteKey: Boolean(row.groupBindingId || row.groupIdHash),
  };
}

function mergeKeyRowsWithSavedGroupOptions(
  rows: StationKeyDraft[],
  groups: StationKeyGroupOption[],
): StationKeyDraft[] {
  return rows.map((row) => {
    if (row.deleteRequested || (!row.groupBindingId && !row.groupIdHash && !row.groupName.trim())) {
      return row;
    }
    const group = findMatchingGroupOption(row, groups);
    if (!group) {
      return row;
    }
    return {
      ...row,
      groupBindingId: group.groupBindingId,
      groupIdHash: group.groupIdHash,
      groupName: group.groupName,
      rateMultiplier:
        group.rateMultiplier === null ? row.rateMultiplier : formatMultiplier(group.rateMultiplier),
      inferredGroupCategory: group.inferredGroupCategory,
      groupCategoryOverride: group.groupCategoryOverride,
    };
  });
}

function mergeGroupRowsWithSavedOptions(
  rows: StationGroupDraft[],
  groups: StationKeyGroupOption[],
): StationGroupDraft[] {
  return dedupeGroupRows(
    rows.map((row) => {
      if (row.deleteRequested) {
        return row;
      }
      const group = groups.find((item) => groupsMatch(row, item));
      if (!group) {
        return row;
      }
      return {
        ...row,
        groupBindingId: group.groupBindingId,
        groupIdHash: group.groupIdHash,
        groupName: group.groupName,
        rateMultiplier: row.rateMultiplier,
        inferredGroupCategory: normalizeGroupCategory(group.inferredGroupCategory) ?? "unknown",
        groupCategoryOverride: group.groupCategoryOverride,
      };
    }),
  );
}

function groupBindingsToCurrentOptions(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
  creditPerCny = 1,
) {
  return buildStationGroupOptionsFromCurrentFactsForSelect(
    buildCurrentStationGroupFacts({ bindings, rates }),
    creditPerCny,
  );
}

function dedupeGroupRows(rows: StationGroupDraft[]): StationGroupDraft[] {
  const mergedRows: StationGroupDraft[] = [];
  rows.forEach((row) => {
    const matchIndex = mergedRows.findIndex((item) => groupRowsRepresentSameGroup(item, row));
    if (matchIndex < 0) {
      mergedRows.push(row);
      return;
    }
    mergedRows[matchIndex] = mergeDuplicateGroupRow(mergedRows[matchIndex], row);
  });
  return mergedRows;
}

function groupRowsRepresentSameGroup(left: StationGroupDraft, right: StationGroupDraft) {
  return Boolean(
    (left.groupBindingId && right.groupBindingId && left.groupBindingId === right.groupBindingId) ||
      (left.groupIdHash && right.groupIdHash && left.groupIdHash === right.groupIdHash) ||
      (left.groupName.trim() &&
        right.groupName.trim() &&
        left.groupName.trim() === right.groupName.trim()),
  );
}

function mergeDuplicateGroupRow(existing: StationGroupDraft, incoming: StationGroupDraft): StationGroupDraft {
  const preferred = preferGroupRow(existing, incoming);
  const fallback = preferred === existing ? incoming : existing;
  return {
    ...preferred,
    clientId: existing.clientId,
    groupBindingId: existing.groupBindingId ?? incoming.groupBindingId,
    groupKeyHash: existing.groupKeyHash || incoming.groupKeyHash,
    groupIdHash: incoming.groupIdHash ?? existing.groupIdHash,
    groupName: incoming.groupName.trim() || existing.groupName,
    rateMultiplier: incoming.rateMultiplier.trim() || existing.rateMultiplier || fallback.rateMultiplier,
    inferredGroupCategory:
      incoming.inferredGroupCategory === "unknown" ? existing.inferredGroupCategory : incoming.inferredGroupCategory,
    groupCategoryOverride: incoming.groupCategoryOverride ?? existing.groupCategoryOverride,
    source: incoming.source === "remote" ? "remote" : existing.source,
    deleteRequested: existing.deleteRequested && incoming.deleteRequested,
  };
}

function preferGroupRow(existing: StationGroupDraft, incoming: StationGroupDraft) {
  if (existing.groupBindingId && !incoming.groupBindingId) {
    return existing;
  }
  if (incoming.groupBindingId && !existing.groupBindingId) {
    return incoming;
  }
  if (incoming.source === "remote" && existing.source !== "remote") {
    return incoming;
  }
  return existing;
}

function rowHasMeaningfulNonSecretContent(row: StationKeyDraft) {
  return Boolean(
    row.name.trim() || row.groupName.trim() || row.rateMultiplier.trim() || row.note.trim(),
  );
}

function parseOptionalRateMultiplier(value: string) {
  if (!value.trim()) {
    return null;
  }
  const rate = Number(value);
  if (!Number.isFinite(rate)) {
    throw new Error("倍率必须是大于等于 0 的有效数字");
  }
  if (rate < 0) {
    throw new Error("倍率不能小于 0");
  }
  return rate;
}

function parseDraftRateMultiplier(value: string) {
  if (!value.trim()) {
    return null;
  }
  const rate = Number(value);
  return Number.isFinite(rate) && rate >= 0 ? rate : null;
}

function validateKeyRows(rows: StationKeyDraft[]) {
  rows
    .filter((row) => !row.deleteRequested)
    .forEach((row) => {
      const hasContent = rowHasMeaningfulContent(row);
      if (!row.id && rowHasMeaningfulNonSecretContent(row) && !row.apiKey.trim()) {
        throw new Error("新增密钥请填写密钥内容，或删除该行。");
      }
      if (hasContent && !row.name.trim()) {
        throw new Error("请填写密钥名称");
      }
      parseOptionalRateMultiplier(row.rateMultiplier);
    });
}

function findReusableDefaultKey(keys: StationKey[]) {
  if (keys.length === 1) {
    return keys[0];
  }
  const defaultKeys = keys.filter((key) => key.priority === 0 && key.name === "Default Key");
  return defaultKeys.length === 1 ? defaultKeys[0] : null;
}

async function saveKeyRows(targetStationId: string, rows: StationKeyDraft[]) {
  validateKeyRows(rows);

  await Promise.all(
    rows
      .filter((row) => row.id && row.deleteRequested)
      .map((row) => deleteStationKey(row.id ?? "")),
  );

  const visibleRows = rows
    .filter((row) => !row.deleteRequested)
    .filter((row) => row.id || row.apiKey.trim());

  for (const [priority, row] of visibleRows.entries()) {
    const rateMultiplier = parseOptionalRateMultiplier(row.rateMultiplier);
    const rateFields = row.rateMultiplier.trim()
      ? { rateMultiplier, rateSource: "manual" as const }
      : {};
    const input = {
      stationId: targetStationId,
      name: row.name.trim(),
      enabled: row.enabled,
      priority,
      groupBindingId: row.groupBindingId,
      groupIdHash: row.groupIdHash,
      groupName: row.groupName.trim() ? row.groupName.trim() : null,
      tierLabel: null,
      balanceScope: "station_key",
      note: row.note.trim() ? row.note.trim() : null,
      ...rateFields,
    };

    if (row.id) {
      await updateStationKey({
        ...input,
        id: row.id,
        apiKey: row.apiKey.trim() ? row.apiKey.trim() : null,
        status: "unchecked",
      });
      continue;
    }

    if (!row.apiKey.trim()) {
      continue;
    }

    await createStationKey({
      ...input,
      apiKey: row.apiKey.trim(),
    });
  }
}

function validateGroupRows(rows: StationGroupDraft[]) {
  rows
    .filter((row) => !row.deleteRequested)
    .filter(groupRowHasMeaningfulContent)
    .forEach((row) => {
      if (!row.groupName.trim()) {
        throw new Error("请填写分组名称");
      }
      parseOptionalRateMultiplier(row.rateMultiplier);
    });
}

async function saveGroupRows(targetStationId: string, rows: StationGroupDraft[], creditPerCny = 1) {
  validateGroupRows(rows);
  const savedOptions: StationKeyGroupOption[] = [];
  const existingBindings = await listStationGroupBindings(targetStationId);

  for (const row of rows) {
    if (!groupRowHasMeaningfulContent(row)) {
      continue;
    }

    if (row.deleteRequested) {
      await disableMatchingGroupBindings(targetStationId, row, existingBindings);
      continue;
    }

    const groupName = row.groupName.trim();
    const groupKeyHash = resolveGroupKeyHash(row);
    const rateMultiplier = parseOptionalRateMultiplier(row.rateMultiplier);
    if (!groupName && !row.groupBindingId) {
      continue;
    }

    const input: UpsertStationGroupBindingInput = {
      stationId: targetStationId,
      stationKeyId: null,
      bindingKind: "station_group",
      parentGroupBindingId: null,
      groupKeyHash,
      groupIdHash: row.groupIdHash,
      groupName: groupName || row.groupName,
      bindingStatus: "available",
      defaultRateMultiplier: row.source === "remote" ? rateMultiplier : null,
      userRateMultiplier: row.source === "manual" ? rateMultiplier : null,
      effectiveRateMultiplier: rateMultiplier,
      inferredGroupCategory: row.inferredGroupCategory,
      groupCategoryOverride: row.groupCategoryOverride,
      rateSource: row.source === "remote" ? "remote_scan" : "manual",
      confidence: row.source === "remote" ? 0.95 : 1,
      lastSeenAt: row.source === "remote" ? new Date().toISOString() : null,
      rawJsonRedacted: null,
    };
    const saved = await upsertStationGroupBinding(input);
    savedOptions.push(buildStationGroupOptionFromRawMultiplierForSelect(saved, creditPerCny));
  }

  return savedOptions;
}

async function disableMatchingGroupBindings(
  targetStationId: string,
  row: StationGroupDraft,
  existingBindings: StationGroupBinding[],
) {
  const bindingsToDisable = existingBindings
    .filter(isCollectedStationGroupBinding)
    .filter((binding) => groupBindingMatchesDraft(binding, row));
  for (const binding of bindingsToDisable) {
    await upsertStationGroupBinding({
      stationId: targetStationId,
      stationKeyId: null,
      bindingKind: "station_group",
      parentGroupBindingId: binding.parentGroupBindingId,
      groupKeyHash: binding.groupKeyHash,
      groupIdHash: binding.groupIdHash,
      groupName: binding.groupName,
      bindingStatus: "disabled",
      defaultRateMultiplier: null,
      userRateMultiplier: null,
      effectiveRateMultiplier: null,
      inferredGroupCategory: binding.inferredGroupCategory,
      groupCategoryOverride: binding.groupCategoryOverride,
      rateSource: binding.rateSource,
      confidence: binding.confidence,
      lastSeenAt: binding.lastSeenAt,
      rawJsonRedacted: binding.rawJsonRedacted,
    });
  }
}

function groupBindingMatchesDraft(binding: StationGroupBinding, row: StationGroupDraft) {
  const rowName = row.groupName.trim();
  return Boolean(
    (row.groupBindingId && binding.id === row.groupBindingId) ||
      (row.groupIdHash && binding.groupIdHash === row.groupIdHash) ||
      (rowName && binding.groupName.trim() === rowName),
  );
}

function resolveGroupKeyHash(row: StationGroupDraft) {
  if (row.groupKeyHash.trim()) {
    return row.groupKeyHash.trim();
  }
  if (row.groupIdHash) {
    return `remote:${row.groupIdHash}`;
  }
  return buildManualGroupKeyHash(row.groupName);
}

function buildManualGroupKeyHash(groupName: string) {
  const normalizedName = groupName.trim().toLowerCase();
  return `manual:${encodeURIComponent(normalizedName || "unnamed")}`;
}

function collectRemoteGroupOptions(remoteKeys: RemoteStationKey[], creditPerCny = 1) {
  const seen = new Set<string>();
  const groups: StationGroupOption[] = [];
  remoteKeys.forEach((key) => {
    if (!key.groupIdHash && !key.groupName) {
      return;
    }
    const groupName = key.groupName?.trim() || "未命名分组";
    const groupKey = `${key.groupIdHash ?? ""}|${groupName}`;
    if (seen.has(groupKey)) {
      return;
    }
    seen.add(groupKey);
    groups.push({
      value: key.groupIdHash ? `remote:${key.groupIdHash}` : `name:${groupName.trim()}`,
      groupBindingId: null,
      groupIdHash: key.groupIdHash,
      groupName,
      rateMultiplier: effectiveRateMultiplierForCredit(key.rateMultiplier, creditPerCny),
      inferredGroupCategory: inferGroupCategoryFromEvidence({ groupName, rawJsonRedacted: null }),
      groupCategoryOverride: null,
      effectiveGroupCategory: inferGroupCategoryFromEvidence({ groupName, rawJsonRedacted: null }),
      rateSource: null,
      selectableForRemoteKey: Boolean(key.groupIdHash),
    });
  });
  return groups;
}

function mergeRemoteGroupOptions(
  editableGroups: StationKeyGroupOption[],
  remoteGroups: ReturnType<typeof collectRemoteGroupOptions>,
) {
  const seen = new Set<string>();
  const groups: ReturnType<typeof collectRemoteGroupOptions> = [];

  function appendGroup(group: StationGroupOption) {
    if (!group.groupIdHash && !group.groupBindingId && !group.groupName.trim()) {
      return;
    }
    const groupName = group.groupName.trim() || "未命名分组";
    const groupKey = groupOptionMergeKey(group, groupName);
    if (seen.has(groupKey)) {
      return;
    }
    seen.add(groupKey);
    groups.push({
      value: group.value || groupKey,
      groupBindingId: group.groupBindingId,
      groupIdHash: group.groupIdHash,
      groupName,
      rateMultiplier: group.rateMultiplier,
      inferredGroupCategory: group.inferredGroupCategory,
      groupCategoryOverride: group.groupCategoryOverride,
      effectiveGroupCategory: group.effectiveGroupCategory,
      rateSource: group.rateSource,
      selectableForRemoteKey: group.selectableForRemoteKey,
    });
  }

  editableGroups.forEach(appendGroup);
  remoteGroups.forEach(appendGroup);
  return groups;
}

function groupOptionMergeKey(
  group: Pick<StationGroupOption, "groupBindingId" | "groupIdHash">,
  groupName: string,
) {
  const groupIdHash = group.groupIdHash?.trim() ?? "";
  if (groupIdHash) {
    return `remote:${groupIdHash}:${groupName}`;
  }

  const groupBindingId = group.groupBindingId?.trim() ?? "";
  if (groupBindingId) {
    return `binding:${groupBindingId}`;
  }

  return `name:${groupName}`;
}

function remoteLocalKeyNote(remoteKey: RemoteStationKey) {
  return `${remoteLocalKeyNotePrefix}：${remoteKey.id}`;
}

function resolveRemoteCreatedLocalKeyIds(
  remoteKeys: RemoteStationKey[],
  localKeys: StationKey[],
) {
  const localKeysByNote = new Map(
    localKeys
      .filter((key) => key.note?.startsWith(remoteLocalKeyNotePrefix))
      .map((key) => [key.note, key.id] as const),
  );

  return Object.fromEntries(
    remoteKeys.flatMap((remoteKey) => {
      const localKeyId = localKeysByNote.get(remoteLocalKeyNote(remoteKey));
      return localKeyId ? [[remoteKey.id, localKeyId] as const] : [];
    }),
  );
}

function remoteKeyDisplayName(remoteKey: RemoteStationKey) {
  return remoteKey.remoteKeyName?.trim() || remoteKey.apiKeyMasked || remoteKey.remoteKeyIdHash || "远端 Key";
}

function groupsMatch(row: StationGroupDraft, group: StationKeyGroupOption) {
  return Boolean(
    (row.groupBindingId && group.groupBindingId === row.groupBindingId) ||
      (row.groupIdHash && group.groupIdHash === row.groupIdHash) ||
      (row.groupName.trim() && group.groupName.trim() === row.groupName.trim()),
  );
}

export function AddProviderPage({ stationId, onBack, onCreated, onUpdated }: AddProviderPageProps) {
  const toast = useToast();
  const editing = Boolean(stationId);
  const [activeStationId, setActiveStationId] = useState<string | null>(stationId ?? null);
  const [form, setForm] = useState<AddProviderFormState>(createDefaultProviderForm);
  const [loading, setLoading] = useState(Boolean(stationId));
  const [saving, setSaving] = useState(false);
  const [testingConnection, setTestingConnection] = useState(false);
  const [startingAuthorization, setStartingAuthorization] = useState(false);
  const [connectionTest, setConnectionTest] = useState<ConnectionTestState>({
    status: "idle",
    message: null,
  });
  const [groupRows, setGroupRows] = useState<StationGroupDraft[]>([]);
  const [currentGroupOptions, setCurrentGroupOptions] = useState<StationKeyGroupOption[]>([]);
  const [keyRows, setKeyRows] = useState<StationKeyDraft[]>([createEmptyStationKeyDraft(0)]);
  const [remoteCapability, setRemoteCapability] = useState<RemoteKeyCapability | null>(
    stationId ? null : draftRemoteCapability(defaultPreset.stationType),
  );
  const [remoteCapabilityError, setRemoteCapabilityError] = useState<string | null>(null);
  const [remoteListError, setRemoteListError] = useState<string | null>(null);
  const [remoteKeys, setRemoteKeys] = useState<RemoteStationKey[]>([]);
  const [localStationKeys, setLocalStationKeys] = useState<StationKey[]>([]);
  const [remoteCreatedLocalKeyIds, setRemoteCreatedLocalKeyIds] = useState<Record<string, string>>({});
  const [remoteLoading, setRemoteLoading] = useState(false);
  const [createRemoteOpen, setCreateRemoteOpen] = useState(false);
  const [developerModeEnabled, setDeveloperModeEnabled] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [initialDraftSnapshot, setInitialDraftSnapshot] = useState(() =>
    serializeProviderDraft(createDefaultProviderForm(), [], [createEmptyStationKeyDraft(0)]),
  );
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const currentCreditPerCny = useMemo(() => parseCreditPerCny(form.creditPerCny), [form.creditPerCny]);
  const hasUnsavedChanges = serializeProviderDraft(form, groupRows, keyRows) !== initialDraftSnapshot;

  const editableGroupOptions = useMemo(() => {
    const deletedCurrentGroups = currentGroupOptions.filter((option) =>
      groupRows.some((row) => row.deleteRequested && groupsMatch(row, option)),
    );
    return normalizeStationGroupOptions([
      ...currentGroupOptions.filter((option) => !deletedCurrentGroups.includes(option)),
      ...groupRows.flatMap((row) => {
        const option = groupDraftToOption(row, currentCreditPerCny);
        return option ? [option] : [];
      }),
    ]);
  }, [currentCreditPerCny, currentGroupOptions, groupRows]);

  const remoteGroupOptions = useMemo(
    () => mergeRemoteGroupOptions(editableGroupOptions, collectRemoteGroupOptions(remoteKeys, currentCreditPerCny)),
    [currentCreditPerCny, editableGroupOptions, remoteKeys],
  );

  const remoteUnsupportedReason = remoteCapability?.unsupportedReason ?? null;
  const remoteCapabilityUnavailableReason = remoteCapabilityError
    ? `远端 Key 能力读取失败：${remoteCapabilityError}`
    : remoteUnsupportedReason;
  const remoteDiscoveryReason =
    remoteCapabilityUnavailableReason ??
    (remoteListError ? `远端 Key 列表读取失败：${remoteListError}` : null);
  const createPageRemoteDraftReady =
    Boolean(form.websiteUrl.trim()) &&
    Boolean(form.apiBaseUrl.trim()) &&
    Boolean(form.loginUsername.trim()) &&
    Boolean(form.loginPassword.trim());
  const scanRemoteDisabled =
    remoteLoading ||
    Boolean(remoteCapabilityError) ||
    (activeStationId ? remoteCapability?.canListRemoteKeys !== true : !createPageRemoteDraftReady);
  const savedStationCreateRemoteUnavailable = activeStationId
    ? remoteCapability?.canCreateRemoteKey !== true
    : false;
  const createPageCreateRemoteUnavailable = activeStationId ? false : !createPageRemoteDraftReady;
  const createRemoteDisabled =
    remoteLoading ||
    Boolean(remoteCapabilityError) ||
    savedStationCreateRemoteUnavailable ||
    createPageCreateRemoteUnavailable;

  useEffect(() => {
    let alive = true;
    void getSettings()
      .then((settings) => {
        if (alive) {
          setDeveloperModeEnabled(settings.developerModeEnabled);
        }
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    setActiveStationId(stationId ?? null);
    if (!stationId) {
      const nextForm = createDefaultProviderForm();
      const nextKeyRows = [createEmptyStationKeyDraft(0)];
      setForm(nextForm);
      setGroupRows([]);
      setCurrentGroupOptions([]);
      setKeyRows(nextKeyRows);
      setLocalStationKeys([]);
      setRemoteCreatedLocalKeyIds({});
      setRemoteCapability(draftRemoteCapability(defaultPreset.stationType));
      setRemoteCapabilityError(null);
      setRemoteListError(null);
      setRemoteKeys([]);
      setCreateRemoteOpen(false);
      setInitialDraftSnapshot(serializeProviderDraft(nextForm, [], nextKeyRows));
      setLoading(false);
      return;
    }

    let alive = true;
    setLoading(true);
    setError(null);
    void Promise.all([
      listStations(),
      getStationCredentials(stationId),
      listStationKeys(stationId),
      listStationGroupBindings(stationId),
      listGroupRateRecords(stationId),
      getRemoteKeyCapability(stationId)
        .then((capability) => ({ capability, error: null }))
        .catch((requestError) => ({ capability: null, error: readError(requestError) })),
      listRemoteStationKeys(stationId)
        .then((keys) => ({ keys, error: null }))
        .catch((requestError) => ({ keys: [], error: readError(requestError) })),
    ])
      .then(([stations, credentials, keys, groupBindings, groupRates, capabilityResult, discoveredRemoteKeysResult]) => {
        if (!alive) {
          return;
        }
        const station = stations.find((item) => item.id === stationId);
        if (!station) {
          throw new Error("未找到要编辑的供应商");
        }
        const nextForm = formFromStation(station, credentials);
        const nextGroupRows = dedupeGroupRows(groupBindingsToDrafts(groupBindings, groupRates));
        const nextKeyRows = keys.length ? keys.map(keyToDraft) : [];
        setForm(nextForm);
        setLocalStationKeys(keys);
        setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(discoveredRemoteKeysResult.keys, keys));
        setCurrentGroupOptions(groupBindingsToCurrentOptions(groupBindings, groupRates, station.creditPerCny));
        setGroupRows(nextGroupRows);
        setKeyRows(nextKeyRows);
        setRemoteCapability(capabilityResult.capability);
        setRemoteCapabilityError(capabilityResult.error);
        setRemoteListError(discoveredRemoteKeysResult.error);
        setRemoteKeys(discoveredRemoteKeysResult.keys);
        setConnectionTest({ status: "idle", message: null });
        setInitialDraftSnapshot(serializeProviderDraft(nextForm, nextGroupRows, nextKeyRows));
      })
      .catch((requestError) => {
        if (!alive) {
          return;
        }
        const message = readError(requestError);
        setError(message);
        toast.error("读取供应商失败", message);
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [stationId, toast]);

  useEffect(() => {
    setKeyRows((currentRows) => syncRowsWithGroupRateOptions(currentRows, editableGroupOptions));
  }, [editableGroupOptions]);

  function applyPreset(presetId: ProviderPresetId) {
    const preset = providerPresets.find((item) => item.id === presetId) ?? defaultPreset;
    setForm((current) => ({
      ...current,
      presetId: preset.id,
      name: getPresetDefaultStationName(preset),
      stationType: preset.stationType,
      websiteUrl: preset.websiteUrl,
      apiBaseUrl: preset.apiBaseUrl,
    }));
    if (!activeStationId) {
      setRemoteCapability(draftRemoteCapability(preset.stationType));
      setRemoteCapabilityError(null);
      setRemoteListError(null);
    }
    setError(null);
    setConnectionTest({ status: "idle", message: null });
  }

  async function refreshLocalStationKeyState(targetStationId: string) {
    const keys = await listStationKeys(targetStationId);
    setLocalStationKeys(keys);
    setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(remoteKeys, keys));
    setKeyRows(keys.length ? keys.map(keyToDraft) : []);
    return keys;
  }

  async function ensureStationForRemoteKeyActions() {
    if (activeStationId) {
      return activeStationId;
    }
    if (!form.name.trim()) {
      throw new Error("请填写供应商名称");
    }
    if (!form.websiteUrl.trim()) {
      throw new Error("请填写前端网址");
    }
    if (!form.apiBaseUrl.trim()) {
      throw new Error("请填写 API Base URL");
    }
    const remoteActionStationType: StationType =
      form.stationType === "custom" || form.stationType === "openai-compatible"
        ? "sub2api"
        : form.stationType;

    const firstKeyDraft = keyRows.find((row) => !row.deleteRequested && row.apiKey.trim());
    const stationApiKey = form.apiKey.trim() || firstKeyDraft?.apiKey.trim() || "";
    const station = await createStation({
      name: form.name.trim(),
      stationType: remoteActionStationType,
      websiteUrl: form.websiteUrl.trim(),
      apiBaseUrl: form.apiBaseUrl.trim(),
      apiKey: stationApiKey,
      collectorProxyMode: form.collectorProxyMode,
      collectorProxyUrl: form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
        ? form.collectorProxyUrl.trim()
        : null,
      enabled: form.enabled,
      creditPerCny: Number(form.creditPerCny),
      lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
        ? Number(form.lowBalanceThresholdCny)
        : null,
      collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
      note: form.note.trim() ? form.note.trim() : null,
    });
    if (remoteActionStationType !== form.stationType) {
      setForm((current) => ({ ...current, stationType: remoteActionStationType }));
    }

    let rowsToSave = keyRows;
    if (!form.apiKey.trim() && firstKeyDraft) {
      const createdKeys = await listStationKeys(station.id);
      const defaultKey = findReusableDefaultKey(createdKeys);
      if (defaultKey) {
        rowsToSave = keyRows.map((row) =>
          row.clientId === firstKeyDraft.clientId
            ? { ...row, id: defaultKey.id, apiKey: "" }
            : row,
        );
      }
    }
    const savedGroupOptions = await saveGroupRows(station.id, groupRows, currentCreditPerCny);
    if (savedGroupOptions.length) {
      setGroupRows((currentRows) => mergeGroupRowsWithSavedOptions(currentRows, savedGroupOptions));
      rowsToSave = mergeKeyRowsWithSavedGroupOptions(rowsToSave, savedGroupOptions);
    }
    await saveKeyRows(station.id, rowsToSave);
    if (form.loginUsername.trim() || form.loginPassword.trim()) {
      await updateStationCredentials({
        stationId: station.id,
        loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
        loginPassword: form.loginPassword.trim() ? form.loginPassword.trim() : null,
        rememberPassword: Boolean(form.loginPassword.trim()),
      });
    }

    setActiveStationId(station.id);
    setLocalStationKeys(await refreshLocalStationKeyState(station.id));
    toast.success("供应商已保存，正在获取远端 Key");
    return station.id;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!form.name.trim()) {
      toast.info("请填写供应商名称");
      return;
    }
    if (!form.websiteUrl.trim()) {
      toast.info("请填写前端网址");
      return;
    }
    if (!form.apiBaseUrl.trim()) {
      toast.info("请填写 API Base URL");
      return;
    }

    try {
      validateGroupRows(groupRows);
      validateKeyRows(keyRows);
    } catch (validationError) {
      toast.info(readError(validationError));
      return;
    }

    setSaving(true);
    setError(null);
    let createdStationId: string | null = null;
    try {
      if (activeStationId) {
        await updateStation({
          id: activeStationId,
          name: form.name.trim(),
          stationType: form.stationType,
          websiteUrl: form.websiteUrl.trim(),
          apiBaseUrl: form.apiBaseUrl.trim(),
          apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
          collectorProxyMode: form.collectorProxyMode,
          collectorProxyUrl: form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
            ? form.collectorProxyUrl.trim()
            : null,
          enabled: form.enabled,
          creditPerCny: Number(form.creditPerCny),
          lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
            ? Number(form.lowBalanceThresholdCny)
            : null,
          collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
          note: form.note.trim() ? form.note.trim() : null,
        });
        const savedGroupOptions = await saveGroupRows(activeStationId, groupRows, currentCreditPerCny);
        const rowsToSave = mergeKeyRowsWithSavedGroupOptions(keyRows, savedGroupOptions);
        setGroupRows((currentRows) => mergeGroupRowsWithSavedOptions(currentRows, savedGroupOptions));
        setKeyRows(rowsToSave);
        await saveKeyRows(activeStationId, rowsToSave);
        await refreshLocalStationKeyState(activeStationId);
        if (form.loginUsername.trim() || form.loginPassword.trim() || form.rememberPassword) {
          await updateStationCredentials({
            stationId: activeStationId,
            loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
            loginPassword: form.loginPassword.trim() ? form.loginPassword.trim() : null,
            rememberPassword: form.rememberPassword,
          });
        }
        toast.success(editing ? "供应商已更新" : "供应商已添加");
        if (editing) {
          onUpdated?.();
        } else {
          onCreated?.();
        }
        return;
      }

      const firstKeyDraft = keyRows.find((row) => !row.deleteRequested && row.apiKey.trim());
      const stationApiKey = form.apiKey.trim() || firstKeyDraft?.apiKey.trim() || "";
      const station = await createStation({
        name: form.name.trim(),
        stationType: form.stationType,
        websiteUrl: form.websiteUrl.trim(),
        apiBaseUrl: form.apiBaseUrl.trim(),
        apiKey: stationApiKey,
        collectorProxyMode: form.collectorProxyMode,
        collectorProxyUrl: form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
          ? form.collectorProxyUrl.trim()
          : null,
        enabled: form.enabled,
        creditPerCny: Number(form.creditPerCny),
        lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
          ? Number(form.lowBalanceThresholdCny)
          : null,
        collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
        note: form.note.trim() ? form.note.trim() : null,
      });
      createdStationId = station.id;
      setActiveStationId(station.id);
      let rowsToSave = keyRows;
      if (!form.apiKey.trim() && firstKeyDraft) {
        const createdKeys = await listStationKeys(station.id);
        const defaultKey = findReusableDefaultKey(createdKeys);
        if (defaultKey) {
          rowsToSave = keyRows.map((row) =>
            row.clientId === firstKeyDraft.clientId
              ? { ...row, id: defaultKey.id, apiKey: "" }
              : row,
          );
        }
      }
      const savedGroupOptions = await saveGroupRows(station.id, groupRows, currentCreditPerCny);
      rowsToSave = mergeKeyRowsWithSavedGroupOptions(rowsToSave, savedGroupOptions);
      setGroupRows((currentRows) => mergeGroupRowsWithSavedOptions(currentRows, savedGroupOptions));
      setKeyRows(rowsToSave);
      await saveKeyRows(station.id, rowsToSave);
      if (form.loginUsername.trim() || form.loginPassword.trim()) {
        await updateStationCredentials({
          stationId: station.id,
          loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
          loginPassword: form.loginPassword.trim() ? form.loginPassword.trim() : null,
          rememberPassword: Boolean(form.loginPassword.trim()),
        });
      }
      toast.success("供应商已添加");
      onCreated?.();
    } catch (requestError) {
      const message = requestError instanceof Error ? requestError.message : String(requestError);
      if (!editing && createdStationId) {
        const partialSuccessMessage = "供应商已创建，但密钥或登录信息保存失败，请重新打开后补全。";
        setError(partialSuccessMessage);
        toast.error("供应商部分保存成功", partialSuccessMessage);
        onCreated?.();
      } else {
        setError(message);
        toast.error(editing ? "保存供应商失败" : "添加供应商失败", message);
      }
    } finally {
      setSaving(false);
    }
  }

  async function handleTestConnection() {
    if (!form.websiteUrl.trim()) {
      toast.info("请填写前端网址");
      return;
    }
    if (!form.loginUsername.trim() || !form.loginPassword.trim()) {
      toast.info("请填写登录用户名和密码");
      return;
    }

    setTestingConnection(true);
    setError(null);
    setConnectionTest({ status: "testing", message: "正在测试连通性..." });
    try {
      const result = await testStationLoginInput({
        stationType: form.stationType,
        websiteUrl: form.websiteUrl.trim(),
        loginUsername: form.loginUsername.trim(),
        loginPassword: form.loginPassword.trim(),
      });
      const message = result.diagnosis
        ? `${result.message} ${result.diagnosis}`
        : result.message;
      if (result.status === "success") {
        setConnectionTest({ status: "success", message });
        toast.success("连通性测试通过", result.message);
      } else {
        setConnectionTest({ status: "warning", message });
        toast.info("连通性测试已完成", result.message);
      }
    } catch (requestError) {
      const message = readError(requestError);
      setConnectionTest({ status: "error", message });
      toast.error("连通性测试失败", message);
    } finally {
      setTestingConnection(false);
    }
  }

  async function handleStartManualAuthorization() {
    if (!activeStationId) {
      toast.info("请先保存供应商后再打开网页登录授权");
      return;
    }

    setStartingAuthorization(true);
    setError(null);
    try {
      await startManualAuthorization(activeStationId);
      toast.success("已打开网页登录授权窗口", "请在弹窗中完成登录，授权成功后会自动写回会话。");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("打开网页登录授权失败", message);
    } finally {
      setStartingAuthorization(false);
    }
  }

  async function handleScanRemoteKeys() {
    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      const targetStationId = await ensureStationForRemoteKeyActions();
      const result = await scanRemoteStationKeys(targetStationId);
      setRemoteCapability(result.capability);
      setRemoteCapabilityError(null);
      setRemoteKeys(result.keys);
      const keys = await refreshLocalStationKeyState(targetStationId);
      setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(result.keys, keys));
      toast.success("远端 Key 已更新", result.message || `发现 ${result.keys.length} 个远端 Key`);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("获取远端 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleSyncRemoteGroups() {
    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      const targetStationId = await ensureStationForRemoteKeyActions();
      await collectStationTask(targetStationId, "groups");
      const [groupBindings, groupRates, capability] = await Promise.all([
        listStationGroupBindings(targetStationId),
        listGroupRateRecords(targetStationId),
        getRemoteKeyCapability(targetStationId).catch(() => null),
      ]);
      const syncedGroupRows = dedupeGroupRows(groupBindingsToDrafts(groupBindings, groupRates));
      setCurrentGroupOptions(groupBindingsToCurrentOptions(groupBindings, groupRates, currentCreditPerCny));
      setRemoteCapability(capability);
      setRemoteCapabilityError(null);
      setGroupRows(syncedGroupRows);
      toast.success("远端分组已同步", `发现 ${syncedGroupRows.length} 个分组，已用远端采集结果覆盖本地编辑区`);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("同步远端分组失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleOpenCreateRemoteKey() {
    if (activeStationId) {
      setCreateRemoteOpen(true);
      return;
    }

    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      const targetStationId = await ensureStationForRemoteKeyActions();
      const capability = await getRemoteKeyCapability(targetStationId);
      setRemoteCapability(capability);
      setRemoteCapabilityError(null);
      if (capability.canCreateRemoteKey !== true) {
        const reason = capability.unsupportedReason ?? "当前中转站暂不支持新建远端 Key";
        setRemoteListError(reason);
        toast.info(reason);
        return;
      }
      setCreateRemoteOpen(true);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("准备新建远端 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  function handleCreateRemoteKey(input: RemoteCreateInput) {
    void submitCreateRemoteKey(input);
  }

  async function submitCreateRemoteKey(input: RemoteCreateInput) {
    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      const targetStationId = await ensureStationForRemoteKeyActions();
      const result = await createRemoteStationKey({
        stationId: targetStationId,
        ...input,
      });
      setRemoteKeys((current) => [
        result.remoteKey,
        ...current.filter(
          (key) =>
            key.id !== result.remoteKey.id &&
            key.remoteKeyIdHash !== result.remoteKey.remoteKeyIdHash,
        ),
      ]);
      await refreshLocalStationKeyState(targetStationId);
      setCreateRemoteOpen(false);
      toast.success("远端 Key 已创建", result.message || "已同步保存为本地 Key");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("创建远端 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleBindRemoteKey(remoteKeyId: string, stationKeyId: string) {
    setRemoteLoading(true);
    setError(null);
    try {
      const keys = await bindRemoteStationKey(remoteKeyId, stationKeyId);
      setRemoteKeys(keys.filter((key) => key.stationId === activeStationId));
      toast.success("远端 Key 已绑定");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("绑定远端 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleRemoteLocalKeyToggle(
    remoteKey: RemoteStationKey,
    checked: boolean,
  ) {
    setRemoteLoading(true);
    setError(null);
    try {
      if (checked) {
        await createLocalKeyFromRemote(remoteKey);
      } else {
        const createdLocalKeyId = remoteCreatedLocalKeyIds[remoteKey.id];
        if (!createdLocalKeyId) {
          toast.info("这条远端 Key 不是由开关创建的本地 Key，未删除");
          return;
        }
        await deleteRemoteCreatedLocalKey(
          remoteKey,
          createdLocalKeyId,
        );
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error(checked ? "创建本地 Key 失败" : "删除本地 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function createLocalKeyFromRemote(remoteKey: RemoteStationKey) {
    const targetStationId = await ensureStationForRemoteKeyActions();
    const result = await createLocalStationKeyFromRemote(remoteKey.id, targetStationId);
    await updateStationKey({
      ...result.stationKey,
      apiKey: null,
      rateMultiplier: effectiveRateMultiplierForCredit(remoteKey.rateMultiplier, currentCreditPerCny),
      note: remoteLocalKeyNote(remoteKey),
    });
    const nextRemoteKeys = (await bindRemoteStationKey(remoteKey.id, result.stationKey.id)).filter(
      (key) => key.stationId === targetStationId,
    );
    const nextLocalKeys = await refreshLocalStationKeyState(targetStationId);
    setRemoteKeys(nextRemoteKeys);
    setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(nextRemoteKeys, nextLocalKeys));
    toast.success("已创建本地 Key", result.message || `${remoteKeyDisplayName(remoteKey)} 已保存为本地 Key。`);
  }

  async function deleteRemoteCreatedLocalKey(
    remoteKey: RemoteStationKey,
    expectedStationKeyId: string,
  ) {
    if (remoteKey.matchedStationKeyId && remoteKey.matchedStationKeyId !== expectedStationKeyId) {
      throw new Error("远端 Key 已匹配到其他本地 Key，未删除。");
    }

    const expectedLocalKey = localStationKeys.find((key) => key.id === expectedStationKeyId);
    if (expectedLocalKey?.note !== remoteLocalKeyNote(remoteKey)) {
      throw new Error("这把本地 Key 不是开关自动创建的，未删除。");
    }

    const nextRemoteKeys = (await unbindRemoteStationKey(remoteKey.id, remoteKey.stationId)).filter(
      (key) => key.stationId === remoteKey.stationId,
    );
    await deleteStationKey(expectedStationKeyId);
    const nextLocalKeys = await refreshLocalStationKeyState(remoteKey.stationId);
    setRemoteKeys(nextRemoteKeys);
    setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(nextRemoteKeys, nextLocalKeys));
    toast.success("已删除自动创建的本地 Key");
  }

  function handleAddLocalKey() {
    setKeyRows((currentRows) => [
      ...currentRows,
      createEmptyStationKeyDraft(currentRows.length),
    ]);
  }

  function requestExit() {
    if (hasUnsavedChanges) {
      setDiscardConfirmOpen(true);
      return;
    }
    onBack();
  }

  return (
    <PageScaffold
      title={editing ? "编辑供应商" : "添加新供应商"}
      stickyHeader
      backAction={
        <IconButton label="返回中转站" onClick={requestExit}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
    >
      <PageForm
        className="w-full"
        onSubmit={handleSubmit}
        footer={
          <>
            <Button variant="secondary" onClick={requestExit} disabled={saving}>
              取消
            </Button>
            <Button type="submit" disabled={saving || loading}>
              <Check className="h-4 w-4" />
              {saving ? "保存中" : editing ? "保存修改" : "添加供应商"}
            </Button>
          </>
        }
      >
        <section className="grid gap-[var(--shell-page-gap)]">
          <div className="grid gap-[var(--shell-page-gap)]">
            {!editing && (
              <SectionCard title="预设供应商">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,9rem),1fr))] gap-2">
                  {providerPresets.map((preset) => {
                    const selected = preset.id === form.presetId;
                    return (
                      <button
                        key={preset.id}
                        type="button"
                        className={cn(
                          "relative flex h-8 min-w-0 cursor-pointer items-center gap-2 rounded-[var(--surface-radius)] px-2.5 text-left text-xs font-medium transition-colors",
                          selected
                            ? "bg-primary-solid text-primary-foreground shadow-sm"
                            : "bg-muted text-muted-foreground hover:bg-hover hover:text-foreground",
                        )}
                        onClick={() => applyPreset(preset.id)}
                        title={preset.description}
                      >
                        <span
                          className={cn(
                            "flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-[5px] bg-surface text-[10px] font-semibold text-muted-foreground",
                            selected && "text-primary",
                          )}
                        >
                          {preset.name.slice(0, 1)}
                        </span>
                        <span className="min-w-0 truncate">{preset.name}</span>
                        {selected && <Check className="ml-auto h-3.5 w-3.5 shrink-0" />}
                      </button>
                    );
                  })}
                </div>
              </SectionCard>
            )}

            <SectionCard title="连接信息">
              <div className="grid gap-3 md:grid-cols-2">
                <Field label="供应商名称">
                  <input
                    className={inputClassName}
                    value={form.name}
                    onChange={(event) => setForm({ ...form, name: event.target.value })}
                    placeholder="例如 我的供应商"
                  />
                </Field>
                <Field label="站点类型">
                  <SelectControl
                    ariaLabel="站点类型"
                    className={inputClassName}
                    value={form.stationType}
                    options={stationTypeOptions}
                    onChange={(stationType) => {
                      setForm({ ...form, stationType });
                      if (!activeStationId) {
                        setRemoteCapability(draftRemoteCapability(stationType));
                        setRemoteCapabilityError(null);
                        setRemoteListError(null);
                      }
                    }}
                  />
                </Field>
              </div>
              <div className="mt-3 grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
                <Field label="前端网址">
                  <input
                    className={inputClassName}
                    value={form.websiteUrl}
                    onChange={(event) => {
                      setForm({ ...form, websiteUrl: event.target.value });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder="https://example.com"
                  />
                </Field>
                <Field label="API Base URL">
                  <input
                    className={inputClassName}
                    value={form.apiBaseUrl}
                    onChange={(event) => {
                      setForm({ ...form, apiBaseUrl: event.target.value });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder="https://api.example.com/v1"
                  />
                </Field>
                <Button
                  variant="outline"
                  className="whitespace-nowrap px-2.5"
                  onClick={() =>
                    setForm((current) => ({
                      ...current,
                      apiBaseUrl: current.websiteUrl,
                    }))
                  }
                >
                  复制前端网址
                </Button>
              </div>
              <div className="mt-3 grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto_auto] md:items-end">
                <Field label="登录用户名 / 邮箱">
                  <input
                    className={inputClassName}
                    value={form.loginUsername}
                    onChange={(event) => {
                      setForm({ ...form, loginUsername: event.target.value });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder="user@example.com"
                  />
                </Field>
                <Field label="登录密码">
                  <input
                    className={inputClassName}
                    type="password"
                    value={form.loginPassword}
                    onChange={(event) => {
                      setForm({
                        ...form,
                        loginPassword: event.target.value,
                        rememberPassword: Boolean(event.target.value.trim()),
                      });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder={editing ? "留空保留旧密码" : "用于采集登录"}
                  />
                </Field>
                <Button
                  variant="outline"
                  onClick={handleTestConnection}
                  disabled={saving || testingConnection}
                >
                  <ShieldCheck className="h-4 w-4" />
                  {testingConnection ? "测试中" : "测试连通性"}
                </Button>
                {editing && (
                  <Button
                    variant="outline"
                    onClick={handleStartManualAuthorization}
                    disabled={saving || loading || startingAuthorization}
                  >
                    <LogIn className="h-4 w-4" />
                    {startingAuthorization ? "打开中" : "网页登录授权"}
                  </Button>
                )}
              </div>
              {connectionTest.message && (
                <div
                  className={cn(
                    "mt-2 min-w-0 truncate text-xs",
                    connectionTest.status === "success" && "text-success-foreground",
                    connectionTest.status === "warning" && "text-warning-foreground",
                    connectionTest.status === "error" && "text-danger-foreground",
                    connectionTest.status === "testing" && "text-muted-foreground",
                  )}
                >
                  {connectionTest.message}
                </div>
              )}
              {error && (
                <div className="mt-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
                  {error}
                </div>
              )}
            </SectionCard>

            <SectionCard
              title="分组"
              action={
                <div className="flex flex-wrap justify-end gap-2">
                  <Button
                    disabled={scanRemoteDisabled}
                    size="sm"
                    title={remoteCapabilityUnavailableReason ?? undefined}
                    variant="outline"
                    onClick={() => void handleSyncRemoteGroups()}
                  >
                    <RefreshCw className={cn("h-3.5 w-3.5", remoteLoading && "animate-spin")} />
                    同步远端分组
                  </Button>
                  <Button
                    disabled={saving || loading}
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      setGroupRows((currentRows) =>
                        dedupeGroupRows([
                          ...currentRows,
                          createEmptyStationGroupDraft(currentRows.length),
                        ]),
                      )
                    }
                  >
                    <Plus className="h-3.5 w-3.5" />
                    添加分组
                  </Button>
                </div>
              }
            >
              <StationGroupRowsEditor
                developerModeEnabled={developerModeEnabled}
                disabled={saving || loading}
                rows={groupRows}
                onRowsChange={(rows) => setGroupRows(dedupeGroupRows(rows))}
              />
            </SectionCard>

            <SectionCard
              title="密钥"
              action={
                <div className="flex flex-wrap justify-end gap-2">
                  <Button
                    disabled={scanRemoteDisabled}
                    size="sm"
                    title={remoteCapabilityUnavailableReason ?? undefined}
                    variant="outline"
                    onClick={() => void handleScanRemoteKeys()}
                  >
                    <RefreshCw className={cn("h-3.5 w-3.5", remoteLoading && "animate-spin")} />
                    获取所有 Key
                  </Button>
                  <Button
                    disabled={createRemoteDisabled}
                    size="sm"
                    title={remoteCapabilityUnavailableReason ?? undefined}
                    variant="secondary"
                    onClick={() => void handleOpenCreateRemoteKey()}
                  >
                    <Plus className="h-3.5 w-3.5" />
                    新建远端 Key
                  </Button>
                  <Button
                    disabled={saving || loading}
                    size="sm"
                    variant="outline"
                    onClick={handleAddLocalKey}
                  >
                    <Plus className="h-3.5 w-3.5" />
                    添加密钥
                  </Button>
                </div>
              }
            >
              <StationKeyRowsEditor
                disabled={saving || loading}
                groupOptions={editableGroupOptions}
                rows={keyRows}
                onRowsChange={setKeyRows}
              />
              {activeStationId && (
                <div className="mt-3 grid gap-2 border-t border-border pt-3">
                  <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                    <KeyRound className="h-3.5 w-3.5" />
                    远端发现
                  </div>
                  {remoteCapabilityError || (remoteUnsupportedReason && remoteCapability?.canListRemoteKeys !== true) ? (
                    <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
                      {remoteDiscoveryReason}
                    </div>
                  ) : (
                    <>
                      <RemoteKeyDiscoveryList
                        creditPerCny={currentCreditPerCny}
                        keys={remoteKeys}
                        loading={remoteLoading}
                        localKeyIdsCreatedByRemote={remoteCreatedLocalKeyIds}
                        localKeys={localStationKeys}
                        onBind={(remoteKeyId, stationKeyId) =>
                          void handleBindRemoteKey(remoteKeyId, stationKeyId)
                        }
                        onLocalKeyToggle={(remoteKey, checked) =>
                          void handleRemoteLocalKeyToggle(remoteKey, checked)
                        }
                      />
                      {remoteListError && (
                        <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
                          {remoteDiscoveryReason}
                        </div>
                      )}
                    </>
                  )}
                </div>
              )}
            </SectionCard>
          </div>

          <aside className="grid content-start gap-[var(--shell-page-gap)]">
            <SectionCard title="可选项">
              <div className="grid gap-3">
                <Field label="低余额阈值 CNY">
                  <input
                    className={inputClassName}
                    min="0"
                    step="0.01"
                    type="number"
                    value={form.lowBalanceThresholdCny}
                    onChange={(event) => setForm({ ...form, lowBalanceThresholdCny: event.target.value })}
                    placeholder="使用全局设置"
                  />
                </Field>
                <Field label="兑换比例">
                  <input
                    className={inputClassName}
                    min="0.01"
                    step="0.01"
                    type="number"
                    value={form.creditPerCny}
                    onChange={(event) => setForm({ ...form, creditPerCny: event.target.value })}
                  />
                </Field>
                <Field label="采集频率 分钟">
                  <input
                    className={inputClassName}
                    min="1"
                    step="1"
                    type="number"
                    value={form.collectionIntervalMinutes}
                    onChange={(event) => setForm({ ...form, collectionIntervalMinutes: event.target.value })}
                    placeholder="5"
                  />
                </Field>
                <Field label="采集代理">
                  <div className="grid gap-2">
                    <SelectControl
                      ariaLabel="站点采集代理"
                      className={inputClassName}
                      value={form.collectorProxyMode}
                      options={Object.entries(stationProxyModeLabels).map(([value, label]) => ({
                        value: value as StationProxyMode,
                        label,
                      }))}
                      onChange={(collectorProxyMode) => {
                        const nextForm = { ...form, collectorProxyMode };
                        setForm(
                          collectorProxyMode === "manual"
                            ? withManualProxyDefault(nextForm)
                            : nextForm,
                        );
                      }}
                    />
                    {form.collectorProxyMode === "manual" && (
                      <input
                        className={inputClassName}
                        placeholder={DEFAULT_MANUAL_PROXY_URL}
                        value={form.collectorProxyUrl}
                        onChange={(event) =>
                          setForm({ ...form, collectorProxyUrl: event.target.value })
                        }
                      />
                    )}
                    <p className="text-xs text-muted-foreground">
                      登录刷新、余额/分组采集、远端 Key 和本地 key 路由都会使用该站点的有效代理。
                    </p>
                  </div>
                </Field>
                <Field label="备注">
                  <textarea
                    className={`${inputClassName} min-h-24 resize-none py-2`}
                    value={form.note}
                    onChange={(event) => setForm({ ...form, note: event.target.value })}
                    placeholder="登录方式、模型限制或计费说明"
                  />
                </Field>
              </div>
            </SectionCard>
          </aside>
        </section>
      </PageForm>
      <CreateRemoteKeyDialog
        groups={remoteGroupOptions}
        open={createRemoteOpen}
        saving={remoteLoading}
        onClose={() => setCreateRemoteOpen(false)}
        onSubmit={handleCreateRemoteKey}
      />
      <ConfirmDialog
        open={discardConfirmOpen}
        title="放弃未保存修改？"
        description={editing ? "当前供应商修改还没有保存，退出后这些修改会丢失。" : "当前新增供应商还没有保存，退出后这些修改会丢失。"}
        confirmLabel="放弃修改"
        cancelLabel="继续编辑"
        onCancel={() => setDiscardConfirmOpen(false)}
        onConfirm={() => {
          setDiscardConfirmOpen(false);
          onBack();
        }}
      />
    </PageScaffold>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

function normalizeCollectionIntervalMinutes(value: string) {
  const interval = Number(value.trim() || "5");
  return Number.isInteger(interval) && interval > 0 ? interval : 5;
}

function parseCreditPerCny(value: string) {
  const parsed = Number(value.trim());
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 1;
}

function syncRowsWithGroupRateOptions(
  rows: StationKeyDraft[],
  groups: StationKeyGroupOption[],
): StationKeyDraft[] {
  let changed = false;
  const nextRows = rows.map((row) => {
    if (row.deleteRequested || (!row.groupBindingId && !row.groupIdHash && !row.groupName.trim())) {
      return row;
    }
    const group = findMatchingGroupOption(row, groups);
    if (!group || group.rateMultiplier === null) {
      return row;
    }
    const nextRateMultiplier = formatMultiplier(group.rateMultiplier);
    if (row.rateMultiplier === nextRateMultiplier && row.groupName === group.groupName) {
      return row;
    }
    changed = true;
    return {
      ...row,
      groupBindingId: group.groupBindingId,
      groupIdHash: group.groupIdHash,
      groupName: group.groupName,
      rateMultiplier: nextRateMultiplier,
    };
  });
  return changed ? nextRows : rows;
}
