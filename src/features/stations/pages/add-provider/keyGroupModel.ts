import { inferGroupCategoryFromEvidence, normalizeGroupCategory } from "@/lib/groupCategories";
import { effectiveRateMultiplierForCredit } from "@/lib/formatters";
import { deriveStationGroupDisplayFacts } from "@/lib/projections/groupFacts";
import {
  isCollectedStationGroupBinding,
  type GroupRateRecord,
  type StationGroupBinding,
  type StationGroupOption,
} from "@/lib/types/groupFacts";
import type {
  RemoteStationKey,
  StationKey,
  UpdateStationKeyInput,
} from "@/lib/types/stationKeys";
import type { StationGroupDraft } from "../../components/StationGroupRowsEditor";
import type { StationKeyDraft, StationKeyGroupOption } from "../../components/StationKeyRowsEditor";
import {
  buildStationGroupOptionFromRawMultiplierForSelect,
  buildStationGroupOptionsFromCurrentFactsForSelect,
  findMatchingGroupOption,
  formatMultiplier,
} from "@/lib/groupOptionViewModels";
import { remoteLocalKeyNotePrefix } from "./formModel";

export const legacyRemoteLocalKeyNote = "由远端站点创建并同步。";

export function keyToDraft(key: StationKey): StationKeyDraft {
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

export function stationKeyToUpdateInput(
  key: StationKey,
  overrides: Partial<UpdateStationKeyInput> = {},
): UpdateStationKeyInput {
  return {
    id: key.id,
    stationId: key.stationId,
    name: key.name,
    apiKey: null,
    enabled: key.enabled,
    priority: key.priority,
    maxConcurrency: key.maxConcurrency,
    loadFactor: key.loadFactor,
    schedulable: key.schedulable,
    groupBindingId: key.groupBindingId,
    groupIdHash: key.groupIdHash,
    groupName: key.groupName,
    tierLabel: key.tierLabel,
    rateMultiplier: key.rateMultiplier,
    manualRateMultiplier: key.manualRateMultiplier,
    rateSource: key.rateSource,
    balanceScope: key.balanceScope,
    status: key.status,
    note: key.note,
    ...overrides,
  };
}

export function groupBindingsToDrafts(
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

export function rowHasMeaningfulContent(row: StationKeyDraft) {
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

export function groupRowHasMeaningfulContent(row: StationGroupDraft) {
  return Boolean(
    row.groupBindingId ||
      row.groupKeyHash.trim() ||
      row.groupIdHash ||
      row.groupName.trim() ||
      row.rateMultiplier.trim(),
  );
}

export function groupDraftToOption(row: StationGroupDraft, creditPerCny = 1): StationKeyGroupOption | null {
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

export function mergeKeyRowsWithSavedGroupOptions(
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

export function mergeGroupRowsWithSavedOptions(
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

export function groupBindingsToCurrentOptions(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
  creditPerCny = 1,
) {
  return buildStationGroupOptionsFromCurrentFactsForSelect(
    deriveStationGroupDisplayFacts({ bindings, rates }),
    creditPerCny,
  );
}

export function dedupeGroupRows(rows: StationGroupDraft[]): StationGroupDraft[] {
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

export function parseOptionalRateMultiplier(value: string) {
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

export function validateKeyRows(rows: StationKeyDraft[]) {
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

export function findReusableDefaultKey(keys: StationKey[]) {
  if (keys.length === 1) {
    return keys[0];
  }
  const defaultKeys = keys.filter((key) => key.priority === 0 && key.name === "Default 密钥");
  return defaultKeys.length === 1 ? defaultKeys[0] : null;
}

export function validateGroupRows(rows: StationGroupDraft[]) {
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

export function collectRemoteGroupOptions(remoteKeys: RemoteStationKey[], creditPerCny = 1) {
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

export function mergeRemoteGroupOptions(
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

export function remoteLocalKeyNote(remoteKey: RemoteStationKey) {
  return `${remoteLocalKeyNotePrefix}：${remoteKey.id}`;
}

export function resolveRemoteCreatedLocalKeyIds(
  remoteKeys: RemoteStationKey[],
  localKeys: StationKey[],
) {
  const localKeysById = new Map(localKeys.map((key) => [key.id, key] as const));
  const localKeysByNote = new Map(
    localKeys.flatMap((key) => (key.note ? [[key.note, key.id] as const] : [])),
  );

  return Object.fromEntries(
    remoteKeys.flatMap((remoteKey) => {
      const localKeyId =
        localKeysByNote.get(remoteLocalKeyNote(remoteKey)) ??
        resolveLegacyRemoteCreatedLocalKeyId(remoteKey, localKeysById);
      return localKeyId ? [[remoteKey.id, localKeyId] as const] : [];
    }),
  );
}

export function deriveRemoteKeyEditorState(
  remoteKeys: RemoteStationKey[],
  localKeys: StationKey[],
  keyRows: StationKeyDraft[],
) {
  const pendingInvalidatedLocalKeyIds = new Set(
    keyRows.flatMap((row) =>
      row.id && row.deleteRequested ? [row.id] : [],
    ),
  );
  const importedLocalKeyIds = resolveRemoteCreatedLocalKeyIds(remoteKeys, localKeys);
  const pendingUnbindRemoteKeyIds = new Set(
    remoteKeys.flatMap((remoteKey) => {
      const linkedLocalKeyId =
        importedLocalKeyIds[remoteKey.id] ?? remoteKey.matchedStationKeyId;
      return linkedLocalKeyId && pendingInvalidatedLocalKeyIds.has(linkedLocalKeyId)
        ? [remoteKey.id]
        : [];
    }),
  );
  const effectiveLocalKeys = localKeys.filter(
    (key) => !pendingInvalidatedLocalKeyIds.has(key.id),
  );
  const effectiveLocalKeyIds = new Set(effectiveLocalKeys.map((key) => key.id));
  const effectiveRemoteKeys = remoteKeys.map((remoteKey) =>
    remoteKey.matchedStationKeyId && !effectiveLocalKeyIds.has(remoteKey.matchedStationKeyId)
      ? {
          ...remoteKey,
          matchStatus: "unbound" as const,
          matchedStationKeyId: null,
          matchConfidence: 0,
        }
      : remoteKey,
  );

  return {
    remoteKeys: effectiveRemoteKeys,
    localKeys: effectiveLocalKeys,
    pendingUnbindRemoteKeyIds,
    localKeyIdsCreatedByRemote: resolveRemoteCreatedLocalKeyIds(
      effectiveRemoteKeys,
      effectiveLocalKeys,
    ),
  };
}

export function collectNewlyDeletedPersistedKeyIds(
  currentRows: StationKeyDraft[],
  nextRows: StationKeyDraft[],
) {
  const currentRowsByClientId = new Map(
    currentRows.map((row) => [row.clientId, row] as const),
  );
  return nextRows.flatMap((row) => {
    const previousRow = currentRowsByClientId.get(row.clientId);
    return row.id && row.deleteRequested && previousRow?.deleteRequested !== true
      ? [row.id]
      : [];
  });
}

export function isRemoteCreatedLocalKey(remoteKey: RemoteStationKey, localKey: StationKey) {
  return (
    localKey.note === remoteLocalKeyNote(remoteKey) ||
    (localKey.note === legacyRemoteLocalKeyNote && remoteKey.matchedStationKeyId === localKey.id)
  );
}

function resolveLegacyRemoteCreatedLocalKeyId(
  remoteKey: RemoteStationKey,
  localKeysById: Map<string, StationKey>,
) {
  if (!remoteKey.matchedStationKeyId) {
    return undefined;
  }
  const matchedLocalKey = localKeysById.get(remoteKey.matchedStationKeyId);
  return matchedLocalKey && isRemoteCreatedLocalKey(remoteKey, matchedLocalKey)
    ? matchedLocalKey.id
    : undefined;
}

export function remoteKeyDisplayName(remoteKey: RemoteStationKey) {
  return remoteKey.remoteKeyName?.trim() || remoteKey.apiKeyMasked || remoteKey.remoteKeyIdHash || "远端 密钥";
}

export function groupsMatch(row: StationGroupDraft, group: StationKeyGroupOption) {
  return Boolean(
    (row.groupBindingId && group.groupBindingId === row.groupBindingId) ||
      (row.groupIdHash && group.groupIdHash === row.groupIdHash) ||
      (row.groupName.trim() && group.groupName.trim() === row.groupName.trim()),
  );
}

export function normalizeCollectionIntervalMinutes(value: string) {
  const interval = Number(value.trim() || "5");
  return Number.isInteger(interval) && interval > 0 ? interval : 5;
}

export function parseCreditPerCny(value: string) {
  const parsed = Number(value.trim());
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 1;
}

export function syncRowsWithGroupRateOptions(
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

export function buildSavedGroupOptionForSelect(binding: StationGroupBinding, creditPerCny = 1) {
  return buildStationGroupOptionFromRawMultiplierForSelect(binding, creditPerCny);
}
