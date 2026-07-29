import type { RemoteKeyCapability, StationCredentials } from "@/lib/types/stationKeys";
import type { Station, StationProxyMode, StationType } from "@/lib/types/stations";
import type { StationGroupDraft } from "../../components/StationGroupRowsEditor";
import type { StationKeyDraft } from "../../components/StationKeyRowsEditor";
import { providerPresets, type ProviderPresetId } from "../../providerPresets";
export type AddProviderFormState = {
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

export type ConnectionTestState = {
  status: "idle" | "testing" | "success" | "warning" | "error";
  message: string | null;
};

export type RemoteCreateInput = {
  name: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupName: string | null;
};

export const defaultPreset = providerPresets[0];

export const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";
export const remoteLocalKeyNotePrefix = "由远端发现开关自动创建";

export function createDefaultProviderForm(): AddProviderFormState {
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

export function serializeProviderDraft(
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

export function getPresetDefaultStationName(preset: (typeof providerPresets)[number]) {
  return preset.id === "custom" ? "" : preset.name;
}

export function draftRemoteCapability(stationType: StationType): RemoteKeyCapability {
  if (stationType === "sub2api" || stationType === "newapi") {
    return {
      stationId: "",
      stationType,
      canListRemoteKeys: true,
      canCreateRemoteKey: true,
      canDeleteRemoteKeys: true,
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
    canDeleteRemoteKeys: false,
    canReadGroups: false,
    requiresManualSession: false,
    unsupportedReason: `当前仅 Sub2API 支持获取远端 Key`,
  };
}

export function formFromStation(station: Station, credentials: StationCredentials): AddProviderFormState {
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
