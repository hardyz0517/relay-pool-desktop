import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { Station } from "@/lib/types/stations";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import { OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS } from "./stationKeyCapabilityDefaults";

export const KEEP_GROUP_BINDING_VALUE = "__keep__";
export const CLEAR_GROUP_BINDING_VALUE = "__clear__";

export type KeyPoolEditForm = {
  id: string;
  stationId: string;
  stationName: string;
  name: string;
  apiKey: string;
  enabled: boolean;
  schedulable: boolean;
  priority: string;
  groupBindingId: string;
  groupName: string;
  tierLabel: string;
  note: string;
  supportsChatCompletions: boolean;
  supportsResponses: boolean;
  supportsEmbeddings: boolean;
  supportsStream: boolean;
  supportsTools: boolean;
  supportsVision: boolean;
  supportsReasoning: boolean;
  modelAllowlist: string;
  modelBlocklist: string;
  preferredModels: string;
  onlyUseAsBackup: boolean;
  routingTags: string;
};

export const emptyEditForm: KeyPoolEditForm = {
  id: "",
  stationId: "",
  stationName: "",
  name: "",
  apiKey: "",
  enabled: true,
  schedulable: true,
  priority: "0",
  groupBindingId: "",
  groupName: "",
  tierLabel: "",
  note: "",
  supportsChatCompletions: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsChatCompletions,
  supportsResponses: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsResponses,
  supportsEmbeddings: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsEmbeddings,
  supportsStream: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsStream,
  supportsTools: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsTools,
  supportsVision: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsVision,
  supportsReasoning: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsReasoning,
  modelAllowlist: "",
  modelBlocklist: "",
  preferredModels: "",
  onlyUseAsBackup: false,
  routingTags: "",
};

export function groupSelectionFromCreateForm(form: KeyPoolEditForm, options: StationGroupOption[]) {
  const groupOption = selectedGroupOption(options, form.groupBindingId);
  if (!groupOption?.groupBindingId) {
    return { kind: "clear" as const };
  }
  return {
    kind: "set" as const,
    groupBindingId: groupOption.groupBindingId,
    groupIdHash: groupOption.groupIdHash,
    groupName: groupOption.groupName,
  };
}

export function groupSelectionFromEditForm(
  form: KeyPoolEditForm,
  sourceItem: KeyPoolItem,
  options: StationGroupOption[],
) {
  if (
    !form.groupBindingId ||
    form.groupBindingId === KEEP_GROUP_BINDING_VALUE ||
    form.groupBindingId === sourceItem.groupBindingId
  ) {
    return { kind: "keep" as const };
  }
  if (form.groupBindingId === CLEAR_GROUP_BINDING_VALUE) {
    return { kind: "clear" as const };
  }
  const groupOption = selectedGroupOption(options, form.groupBindingId);
  return {
    kind: "set" as const,
    groupBindingId: groupOption?.groupBindingId ?? form.groupBindingId,
    groupIdHash: groupOption?.groupIdHash ?? null,
    groupName: groupOption?.groupName ?? null,
  };
}

export function capabilitiesFromEditForm(form: KeyPoolEditForm) {
  return {
    stationKeyId: form.id,
    supportsChatCompletions: form.supportsChatCompletions,
    supportsResponses: form.supportsResponses,
    supportsEmbeddings: form.supportsEmbeddings,
    supportsStream: form.supportsStream,
    supportsTools: form.supportsTools,
    supportsVision: form.supportsVision,
    supportsReasoning: form.supportsReasoning,
    modelAllowlist: linesToList(form.modelAllowlist),
    modelBlocklist: linesToList(form.modelBlocklist),
    preferredModels: linesToList(form.preferredModels),
    onlyUseAsBackup: form.onlyUseAsBackup,
    routingTags: commaListToList(form.routingTags),
  };
}

export function selectedGroupOption(options: StationGroupOption[], value: string) {
  return options.find((option) => option.groupBindingId === value || option.value === value) ?? null;
}

export function groupNameForDialogSelection(
  value: string,
  sourceItem: KeyPoolItem | null,
  options: StationGroupOption[],
  fallback: string,
) {
  if (!value) {
    return "";
  }
  if (value === KEEP_GROUP_BINDING_VALUE) {
    return sourceItem?.groupName ?? fallback;
  }
  if (value === CLEAR_GROUP_BINDING_VALUE) {
    return "";
  }
  if (value === sourceItem?.groupBindingId) {
    return sourceItem.groupName ?? fallback;
  }
  return selectedGroupOption(options, value)?.groupName ?? fallback;
}

export function formFromItem(item: KeyPoolItem, options: StationGroupOption[] = []): KeyPoolEditForm {
  return {
    id: item.id,
    stationId: item.stationId,
    stationName: item.stationName,
    name: item.name,
    apiKey: "",
    enabled: item.enabled,
    schedulable: item.schedulable,
    priority: String(item.priority),
    groupBindingId: groupBindingValueFromItem(item, options),
    groupName: item.groupName ?? "",
    tierLabel: item.tierLabel ?? "",
    note: item.note ?? "",
    supportsChatCompletions: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsChatCompletions,
    supportsResponses: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsResponses,
    supportsEmbeddings: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsEmbeddings,
    supportsStream: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsStream,
    supportsTools: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsTools,
    supportsVision: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsVision,
    supportsReasoning: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsReasoning,
    modelAllowlist: "",
    modelBlocklist: "",
    preferredModels: "",
    onlyUseAsBackup: item.onlyUseAsBackup,
    routingTags: "",
  };
}

export function groupBindingValueFromItem(item: KeyPoolItem, options: StationGroupOption[]) {
  const option = findMatchingKeyPoolGroupOption(
    {
      groupBindingId: item.groupBindingId,
      groupIdHash: item.groupIdHash,
      groupName: item.groupName ?? "",
    },
    options,
  );
  return option?.groupBindingId ?? item.groupBindingId ?? KEEP_GROUP_BINDING_VALUE;
}

function findMatchingKeyPoolGroupOption(
  row: { groupBindingId: string | null; groupIdHash: string | null; groupName: string },
  options: StationGroupOption[],
) {
  const groupBindingId = row.groupBindingId?.trim() ?? "";
  if (groupBindingId) {
    const bindingMatch = options.find((option) => option.groupBindingId === groupBindingId);
    if (bindingMatch) {
      return bindingMatch;
    }
  }

  const groupIdHash = row.groupIdHash?.trim() ?? "";
  if (groupIdHash) {
    const groupIdMatch = options.find((option) => option.groupIdHash === groupIdHash);
    if (groupIdMatch) {
      return groupIdMatch;
    }
  }

  const groupName = row.groupName.trim();
  if (!groupName) {
    return null;
  }
  return options.find((option) => option.groupName.trim() === groupName) ?? null;
}

export function createFormForStation(station: Station, items: KeyPoolItem[]): KeyPoolEditForm {
  const nextIndex = items.filter((item) => item.stationId === station.id).length;
  return {
    ...emptyEditForm,
    stationId: station.id,
    stationName: station.name,
    name: `${station.name} 密钥 ${nextIndex + 1}`,
    priority: String(nextIndex),
  };
}

export function mergeCapabilitiesIntoForm(
  form: KeyPoolEditForm,
  capabilities: StationKeyCapabilities,
): KeyPoolEditForm {
  return {
    ...form,
    supportsChatCompletions: capabilities.supportsChatCompletions,
    supportsResponses: capabilities.supportsResponses,
    supportsEmbeddings: capabilities.supportsEmbeddings,
    supportsStream: capabilities.supportsStream,
    supportsTools: capabilities.supportsTools,
    supportsVision: capabilities.supportsVision,
    supportsReasoning: capabilities.supportsReasoning,
    modelAllowlist: capabilities.modelAllowlist.join("\n"),
    modelBlocklist: capabilities.modelBlocklist.join("\n"),
    preferredModels: capabilities.preferredModels.join("\n"),
    onlyUseAsBackup: capabilities.onlyUseAsBackup,
    routingTags: capabilities.routingTags.join(", "),
  };
}

export function linesToList(value: string) {
  return Array.from(
    new Set(
      value
        .split(/\r?\n/)
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}

export function commaListToList(value: string) {
  return Array.from(
    new Set(
      value
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}
