import type {
  CreateStationKeyInput,
  StationKey,
  StationKeyStatus,
  UpdateStationKeyInput,
} from "@/lib/types/stationKeys";
import type { Station, StationInput, StationType } from "@/lib/types/stations";

export type StationFormState = {
  name: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
  apiKey: string;
  enabled: boolean;
  creditPerCny: string;
  collectionIntervalMinutes: string;
  note: string;
  loginUsername: string;
  loginPassword: string;
  rememberPassword: boolean;
};

export type StationKeyFormState = {
  id: string | null;
  name: string;
  apiKey: string;
  enabled: boolean;
  priority: string;
  groupName: string;
  tierLabel: string;
  status: StationKeyStatus;
  note: string;
};

export const emptyForm: StationFormState = {
  name: "",
  stationType: "sub2api",
  websiteUrl: "",
  apiBaseUrl: "",
  apiKey: "",
  enabled: true,
  creditPerCny: "1",
  collectionIntervalMinutes: "5",
  note: "",
  loginUsername: "",
  loginPassword: "",
  rememberPassword: false,
};

export const emptyKeyForm: StationKeyFormState = {
  id: null,
  name: "",
  apiKey: "",
  enabled: true,
  priority: "0",
  groupName: "",
  tierLabel: "",
  status: "unchecked",
  note: "",
};

export function formToInput(form: StationFormState): StationInput {
  return {
    name: form.name.trim(),
    stationType: form.stationType,
    websiteUrl: form.websiteUrl.trim(),
    apiBaseUrl: form.apiBaseUrl.trim(),
    apiKey: form.apiKey.trim(),
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    enabled: form.enabled,
    creditPerCny: Number(form.creditPerCny),
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
    note: form.note.trim() ? form.note.trim() : null,
  };
}

export function normalizeCollectionIntervalMinutes(value: string) {
  const interval = Number(value.trim() || "5");
  return Number.isInteger(interval) && interval > 0 ? interval : 5;
}

export function toCreateKeyInput(form: StationKeyFormState, stationId: string): CreateStationKeyInput {
  return {
    stationId,
    name: form.name.trim(),
    apiKey: form.apiKey.trim(),
    enabled: form.enabled,
    priority: Number(form.priority),
    groupName: form.groupName.trim() ? form.groupName.trim() : null,
    tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
    note: form.note.trim() ? form.note.trim() : null,
  };
}

export function toUpdateKeyInput(form: StationKeyFormState, stationId: string): UpdateStationKeyInput {
  return {
    id: form.id ?? "",
    stationId,
    name: form.name.trim(),
    apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
    enabled: form.enabled,
    priority: Number(form.priority),
    groupName: form.groupName.trim() ? form.groupName.trim() : null,
    tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
    status: form.status,
    note: form.note.trim() ? form.note.trim() : null,
  };
}

export function keyToForm(key: StationKey): StationKeyFormState {
  return {
    id: key.id,
    name: key.name,
    apiKey: "",
    enabled: key.enabled,
    priority: String(key.priority),
    groupName: key.groupName ?? "",
    tierLabel: key.tierLabel ?? "",
    status: key.status,
    note: key.note ?? "",
  };
}

export function stationEndpointOriginWarnings(station: Station, form: StationFormState) {
  const warnings: string[] = [];
  if (endpointOriginKey(station.websiteUrl) !== endpointOriginKey(form.websiteUrl)) {
    warnings.push("前端网址 origin 变化后，保存的登录状态会被清除。");
  }
  if (endpointOriginKey(station.apiBaseUrl) !== endpointOriginKey(form.apiBaseUrl)) {
    warnings.push("API origin 变化后，站点会被禁用，现有 Key 将不会路由，直到重新验证并启用。");
  }
  return warnings;
}

function endpointOriginKey(value: string) {
  try {
    const url = new URL(value.trim());
    return `${url.protocol}//${url.host}`;
  } catch {
    return value.trim().replace(/\/+$/, "");
  }
}
