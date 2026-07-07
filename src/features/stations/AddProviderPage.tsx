import { useEffect, useMemo, useState, type FormEvent } from "react";
import { ArrowLeft, Check, KeyRound, Plus, RefreshCw, ShieldCheck } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, PageForm, SectionCard, SelectControl, useToast } from "@/components/ui";
import { collectStationTask, testStationLoginInput } from "@/lib/api/collector";
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
import { readError } from "@/lib/errors";
import {
  isCollectedStationGroupBinding,
  type GroupRateRecord,
  type StationGroupBinding,
  type StationGroupOption,
  type UpsertStationGroupBindingInput,
} from "@/lib/types/groupFacts";
import type { RemoteKeyCapability, RemoteStationKey, StationCredentials, StationKey } from "@/lib/types/stationKeys";
import { stationTypeOptions, type Station, type StationType } from "@/lib/types/stations";
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
  findMatchingGroupOption,
  formatMultiplier,
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
  baseUrl: string;
  apiKey: string;
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

const defaultPreset = providerPresets[0];

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";
const remoteLocalKeyNotePrefix = "由远端发现开关自动创建";

function getPresetDefaultStationName(preset: (typeof providerPresets)[number]) {
  return preset.id === "custom" ? "" : preset.name;
}

function draftRemoteCapability(stationType: StationType): RemoteKeyCapability {
  if (stationType === "sub2api") {
    return {
      stationId: "",
      stationType,
      canListRemoteKeys: true,
      canCreateRemoteKey: false,
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
    baseUrl: station.baseUrl,
    apiKey: "",
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
  return {
    clientId: binding.id,
    groupBindingId: binding.id,
    groupKeyHash: binding.groupKeyHash,
    groupIdHash: binding.groupIdHash,
    groupName: binding.groupName || latestRate?.groupName || "",
    rateMultiplier: rateMultiplier == null ? "" : String(rateMultiplier),
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

function groupDraftToOption(row: StationGroupDraft): StationKeyGroupOption | null {
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
    rateMultiplier: parseDraftRateMultiplier(row.rateMultiplier),
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
      rateMultiplier: group.rateMultiplier === null ? row.rateMultiplier : formatMultiplier(group.rateMultiplier),
    };
  });
}

function mergeGroupRowsWithSavedOptions(
  rows: StationGroupDraft[],
  groups: StationKeyGroupOption[],
): StationGroupDraft[] {
  return dedupeGroupRows(rows.map((row) => {
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
      rateMultiplier: group.rateMultiplier === null ? row.rateMultiplier : formatMultiplier(group.rateMultiplier),
    };
  }));
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
    throw new Error("密钥倍率必须是大于 0 的有效数字");
  }
  if (rate <= 0) {
    throw new Error("密钥倍率必须大于 0");
  }
  return rate;
}

function parseDraftRateMultiplier(value: string) {
  if (!value.trim()) {
    return null;
  }
  const rate = Number(value);
  return Number.isFinite(rate) && rate > 0 ? rate : null;
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

async function saveGroupRows(targetStationId: string, rows: StationGroupDraft[]) {
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
      rateSource: row.source === "remote" ? "remote_scan" : "manual",
      confidence: row.source === "remote" ? 0.95 : 1,
      lastSeenAt: row.source === "remote" ? new Date().toISOString() : null,
      rawJsonRedacted: null,
    };
    const saved = await upsertStationGroupBinding(input);
    savedOptions.push({
      value: saved.id
        ? `binding:${saved.id}`
        : saved.groupIdHash
          ? `remote:${saved.groupIdHash}`
          : `name:${saved.groupName.trim()}`,
      groupBindingId: saved.id,
      groupIdHash: saved.groupIdHash,
      groupName: saved.groupName,
      rateMultiplier:
        saved.userRateMultiplier ?? saved.effectiveRateMultiplier ?? saved.defaultRateMultiplier,
      rateSource: saved.rateSource,
      selectableForRemoteKey: Boolean(saved.id || saved.groupIdHash),
    });
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

function collectRemoteGroupOptions(remoteKeys: RemoteStationKey[]) {
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
      rateMultiplier: key.rateMultiplier,
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
  const [form, setForm] = useState<AddProviderFormState>({
    presetId: defaultPreset.id,
    name: getPresetDefaultStationName(defaultPreset),
    stationType: defaultPreset.stationType,
    baseUrl: defaultPreset.baseUrl,
    apiKey: "",
    enabled: true,
    creditPerCny: "1",
    loginUsername: "",
    loginPassword: "",
    rememberPassword: false,
    lowBalanceThresholdCny: "",
    collectionIntervalMinutes: "5",
    note: "",
  });
  const [loading, setLoading] = useState(Boolean(stationId));
  const [saving, setSaving] = useState(false);
  const [testingConnection, setTestingConnection] = useState(false);
  const [connectionTest, setConnectionTest] = useState<ConnectionTestState>({
    status: "idle",
    message: null,
  });
  const [groupRows, setGroupRows] = useState<StationGroupDraft[]>([]);
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
  const [error, setError] = useState<string | null>(null);

  const editableGroupOptions = useMemo(
    () =>
      groupRows.flatMap((row) => {
        const option = groupDraftToOption(row);
        return option ? [option] : [];
      }),
    [groupRows],
  );

  const remoteGroupOptions = useMemo(
    () => mergeRemoteGroupOptions(editableGroupOptions, collectRemoteGroupOptions(remoteKeys)),
    [editableGroupOptions, remoteKeys],
  );

  const remoteUnsupportedReason = remoteCapability?.unsupportedReason ?? null;
  const remoteCapabilityUnavailableReason = remoteCapabilityError
    ? `远端 Key 能力读取失败：${remoteCapabilityError}`
    : remoteUnsupportedReason;
  const remoteDiscoveryReason =
    remoteCapabilityUnavailableReason ??
    (remoteListError ? `远端 Key 列表读取失败：${remoteListError}` : null);
  const createPageRemoteDraftReady =
    Boolean(form.baseUrl.trim()) && Boolean(form.loginUsername.trim()) && Boolean(form.loginPassword.trim());
  const scanRemoteDisabled =
    remoteLoading ||
    Boolean(remoteCapabilityError) ||
    (activeStationId ? remoteCapability?.canListRemoteKeys !== true : !createPageRemoteDraftReady);
  const createRemoteDisabled =
    remoteLoading ||
    Boolean(remoteCapabilityError) ||
    (activeStationId ? remoteCapability?.canCreateRemoteKey !== true : !createPageRemoteDraftReady);

  useEffect(() => {
    setActiveStationId(stationId ?? null);
    if (!stationId) {
      setGroupRows([]);
      setKeyRows([createEmptyStationKeyDraft(0)]);
      setLocalStationKeys([]);
      setRemoteCreatedLocalKeyIds({});
      setRemoteCapability(draftRemoteCapability(defaultPreset.stationType));
      setRemoteCapabilityError(null);
      setRemoteListError(null);
      setRemoteKeys([]);
      setCreateRemoteOpen(false);
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
        setForm(formFromStation(station, credentials));
        setLocalStationKeys(keys);
        setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(discoveredRemoteKeysResult.keys, keys));
        setGroupRows(dedupeGroupRows(groupBindingsToDrafts(groupBindings, groupRates)));
        setKeyRows(keys.length ? keys.map(keyToDraft) : []);
        setRemoteCapability(capabilityResult.capability);
        setRemoteCapabilityError(capabilityResult.error);
        setRemoteListError(discoveredRemoteKeysResult.error);
        setRemoteKeys(discoveredRemoteKeysResult.keys);
        setConnectionTest({ status: "idle", message: null });
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
      baseUrl: preset.baseUrl,
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
    if (!form.baseUrl.trim()) {
      throw new Error("请填写基础地址");
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
      baseUrl: form.baseUrl.trim(),
      apiKey: stationApiKey,
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
    const savedGroupOptions = await saveGroupRows(station.id, groupRows);
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
    if (!form.baseUrl.trim()) {
      toast.info("请填写基础地址");
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
          baseUrl: form.baseUrl.trim(),
          apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
          enabled: form.enabled,
          creditPerCny: Number(form.creditPerCny),
          lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
            ? Number(form.lowBalanceThresholdCny)
            : null,
          collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
          note: form.note.trim() ? form.note.trim() : null,
        });
        const savedGroupOptions = await saveGroupRows(activeStationId, groupRows);
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
        baseUrl: form.baseUrl.trim(),
        apiKey: stationApiKey,
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
      const savedGroupOptions = await saveGroupRows(station.id, groupRows);
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
    if (!form.baseUrl.trim()) {
      toast.info("请填写基础地址");
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
        baseUrl: form.baseUrl.trim(),
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

  async function handleCreateRemoteKey(input: {
    name: string;
    groupBindingId: string | null;
    groupIdHash: string | null;
    groupName: string | null;
  }) {
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

  return (
    <PageScaffold
      title={editing ? "编辑供应商" : "添加新供应商"}
      stickyHeader
      backAction={
        <IconButton label="返回中转站" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
    >
      <PageForm
        className="w-full"
        onSubmit={handleSubmit}
        footer={
          <>
            <Button variant="secondary" onClick={onBack} disabled={saving}>
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
                            ? "bg-[hsl(var(--accent))] text-white shadow-sm"
                            : "bg-slate-100 text-slate-600 hover:bg-slate-200 hover:text-slate-900",
                        )}
                        onClick={() => applyPreset(preset.id)}
                        title={preset.description}
                      >
                        <span
                          className={cn(
                            "flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-[5px] bg-white text-[10px] font-semibold text-slate-600",
                            selected && "text-[hsl(var(--accent))]",
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
                    placeholder="例如 DeepSeek"
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
              <div className="mt-3 grid gap-3">
                <Field label="基础地址">
                  <input
                    className={inputClassName}
                    value={form.baseUrl}
                    onChange={(event) => {
                      setForm({ ...form, baseUrl: event.target.value });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder="https://api.example.com/v1"
                  />
                </Field>
              </div>
              <div className="mt-3 grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
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
              </div>
              {connectionTest.message && (
                <div
                  className={cn(
                    "mt-2 min-w-0 truncate text-xs",
                    connectionTest.status === "success" && "text-emerald-600",
                    connectionTest.status === "warning" && "text-amber-600",
                    connectionTest.status === "error" && "text-rose-600",
                    connectionTest.status === "testing" && "text-slate-500",
                  )}
                >
                  {connectionTest.message}
                </div>
              )}
              {error && (
                <div className="mt-3 rounded-[var(--surface-radius)] border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-700">
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
                  <div className="flex items-center gap-2 text-xs font-medium text-slate-600">
                    <KeyRound className="h-3.5 w-3.5" />
                    远端发现
                  </div>
                  {remoteCapabilityError || (remoteUnsupportedReason && remoteCapability?.canListRemoteKeys !== true) ? (
                    <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
                      {remoteDiscoveryReason}
                    </div>
                  ) : (
                    <>
                      <RemoteKeyDiscoveryList
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
                        <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
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
        onSubmit={(input) => void handleCreateRemoteKey(input)}
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
