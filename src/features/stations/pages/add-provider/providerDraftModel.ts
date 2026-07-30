import { normalizeGroupCategory } from "@/lib/groupCategories";
import type {
  ProviderDraft,
  ProviderDraftPayload,
  ProviderDraftPreviewGroup,
} from "@/lib/types/providerDrafts";
import type { StationGroupDraft } from "../../components/StationGroupRowsEditor";
import type { StationKeyDraft } from "../../components/StationKeyRowsEditor";
import { providerPresets } from "../../providerPresets";
import type { AddProviderFormState } from "./formModel";
import { dedupeGroupRows, parseOptionalRateMultiplier } from "./keyGroupModel";

export function providerDraftPayloadFromEditor(
  form: AddProviderFormState,
  groupRows: StationGroupDraft[],
  keyRows: StationKeyDraft[],
): ProviderDraftPayload {
  const visibleGroups = groupRows.filter((row) => !row.deleteRequested && row.groupName.trim());
  const groupClientIds = new Set(visibleGroups.map((row) => row.clientId));
  return {
    name: form.name.trim(),
    stationType: form.stationType,
    websiteUrl: form.websiteUrl.trim(),
    apiBaseUrl: form.apiBaseUrl.trim(),
    collectorProxyMode: form.collectorProxyMode,
    collectorProxyUrl:
      form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
        ? form.collectorProxyUrl.trim()
        : null,
    enabled: form.enabled,
    creditPerCny: positiveNumber(form.creditPerCny, 1),
    lowBalanceThresholdCny: optionalNumber(form.lowBalanceThresholdCny),
    collectionIntervalMinutes: Math.max(1, Math.round(positiveNumber(form.collectionIntervalMinutes, 5))),
    note: form.note.trim() || null,
    loginUsername: form.loginUsername.trim() || null,
    rememberPassword: form.rememberPassword,
    groups: visibleGroups.map((row) => ({
      clientId: row.clientId,
      groupKeyHash: resolveGroupKeyHash(row),
      groupIdHash: row.groupIdHash,
      groupName: row.groupName.trim(),
      rateMultiplier: parseOptionalRateMultiplier(row.rateMultiplier),
      inferredGroupCategory: row.inferredGroupCategory,
      groupCategoryOverride: row.groupCategoryOverride,
      source: row.source,
    })),
    keys: keyRows
      .filter((row) => !row.deleteRequested && (row.apiKey.trim() || row.id || row.name.trim()))
      .map((row, index) => ({
        clientId: row.clientId,
        name: row.name.trim() || `Key ${index + 1}`,
        enabled: row.enabled,
        groupClientId:
          row.groupBindingId && groupClientIds.has(row.groupBindingId) ? row.groupBindingId : null,
        groupIdHash: row.groupIdHash,
        groupName: row.groupName.trim() || null,
        rateMultiplier: parseOptionalRateMultiplier(row.rateMultiplier),
        note: row.note.trim() || null,
      })),
  };
}

export function editorFromProviderDraft(draft: ProviderDraft): {
  form: AddProviderFormState;
  groupRows: StationGroupDraft[];
  keyRows: StationKeyDraft[];
} {
  const payload = draft.payload;
  const preset =
    providerPresets.find(
      (item) =>
        item.websiteUrl === payload.websiteUrl && item.apiBaseUrl === payload.apiBaseUrl,
    ) ?? providerPresets[0];
  return {
    form: {
      presetId: preset.id,
      name: payload.name,
      stationType: payload.stationType as AddProviderFormState["stationType"],
      websiteUrl: payload.websiteUrl,
      apiBaseUrl: payload.apiBaseUrl,
      apiKey: "",
      collectorProxyMode: payload.collectorProxyMode as AddProviderFormState["collectorProxyMode"],
      collectorProxyUrl: payload.collectorProxyUrl ?? "",
      enabled: payload.enabled,
      creditPerCny: String(payload.creditPerCny),
      loginUsername: payload.loginUsername ?? "",
      loginPassword: "",
      rememberPassword: payload.rememberPassword,
      lowBalanceThresholdCny:
        payload.lowBalanceThresholdCny === null ? "" : String(payload.lowBalanceThresholdCny),
      collectionIntervalMinutes: String(payload.collectionIntervalMinutes),
      note: payload.note ?? "",
    },
    groupRows: payload.groups.map((group) => ({
      clientId: group.clientId,
      groupBindingId: group.clientId,
      groupKeyHash: group.groupKeyHash,
      groupIdHash: group.groupIdHash,
      groupName: group.groupName,
      rateMultiplier: group.rateMultiplier === null ? "" : String(group.rateMultiplier),
      inferredGroupCategory: normalizeGroupCategory(group.inferredGroupCategory) ?? "unknown",
      groupCategoryOverride: normalizeGroupCategory(group.groupCategoryOverride),
      source: group.source === "remote" ? "remote" : "manual",
      deleteRequested: false,
    })),
    keyRows: payload.keys.map((key) => ({
      clientId: key.clientId,
      id: draft.keyApiKeyClientIds.includes(key.clientId) ? key.clientId : null,
      name: key.name,
      apiKey: "",
      groupBindingId: key.groupClientId,
      groupIdHash: key.groupIdHash,
      groupName: key.groupName ?? "",
      rateMultiplier: key.rateMultiplier === null ? "" : String(key.rateMultiplier),
      enabled: key.enabled,
      note: key.note ?? "",
      deleteRequested: false,
    })),
  };
}

export function mergeProviderDraftPreviewGroups(
  current: StationGroupDraft[],
  previewGroups: ProviderDraftPreviewGroup[],
) {
  const previewRows = previewGroups.map((group) => ({
    clientId: `draft-preview:${group.groupKeyHash}`,
    groupBindingId: null,
    groupKeyHash: group.groupKeyHash,
    groupIdHash: group.groupIdHash,
    groupName: group.groupName,
    rateMultiplier: group.rateMultiplier === null ? "" : String(group.rateMultiplier),
    inferredGroupCategory: normalizeGroupCategory(group.inferredGroupCategory) ?? "unknown",
    groupCategoryOverride: null,
    source: "remote" as const,
    deleteRequested: false,
  }));
  return dedupeGroupRows([...current.filter((row) => row.source !== "remote"), ...previewRows]);
}

function resolveGroupKeyHash(row: StationGroupDraft) {
  if (row.groupKeyHash.trim()) return row.groupKeyHash.trim();
  if (row.groupIdHash) return `remote:${row.groupIdHash}`;
  return `manual:${encodeURIComponent(row.groupName.trim().toLowerCase() || "unnamed")}`;
}

function positiveNumber(value: string, fallback: number) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function optionalNumber(value: string) {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}
