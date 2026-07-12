import {
  SCHEDULER_ADVANCED_FIELD_KINDS,
  type AppSettings,
  type SchedulerAdvancedFieldKind,
} from "@/lib/types/settings";
import type {
  PricingGroupType,
  RoutingGroupFilter,
  SchedulerAdvancedSettings,
} from "@/lib/types/routing";

export type RoutingGroupPreset =
  | "all_groups"
  | "ungrouped_only"
  | PricingGroupType
  | "current_specific";

export type SchedulerNumericField = {
  [Key in keyof SchedulerAdvancedSettings]: SchedulerAdvancedSettings[Key] extends number
    ? Key
    : never;
}[keyof SchedulerAdvancedSettings];

export type SchedulerBooleanField = Exclude<keyof SchedulerAdvancedSettings, SchedulerNumericField>;

export type SchedulerDraft = {
  [Key in keyof SchedulerAdvancedSettings]: SchedulerAdvancedSettings[Key] extends number
    ? string
    : boolean;
};

export type LocalRoutingSettingsDraft = {
  maxRateMultiplier: string;
  defaultRoutingGroupPreset: RoutingGroupPreset;
  currentRoutingGroupFilter: RoutingGroupFilter;
  scheduler: SchedulerDraft;
};

export type LocalRoutingSettingsErrorKey =
  | "maxRateMultiplier"
  | "baseWeights"
  | keyof SchedulerAdvancedSettings;

export type LocalRoutingSettingsErrors = Partial<Record<LocalRoutingSettingsErrorKey, string>>;

export type LocalRoutingSettingsValue = {
  maxRateMultiplier: number | null;
  defaultRoutingGroupFilter: RoutingGroupFilter;
  schedulerAdvancedSettings: SchedulerAdvancedSettings;
};

export type ParsedLocalRoutingSettingsDraft =
  | { ok: true; value: LocalRoutingSettingsValue }
  | { ok: false; errors: LocalRoutingSettingsErrors };

export type SchedulerFieldGroup = "score" | "sticky" | "waiting" | "boundary";

type SchedulerFieldMeta = {
  label: string;
  group: SchedulerFieldGroup;
  step: string;
  max?: string;
};

export const SCHEDULER_NUMERIC_FIELD_META = {
  topK: { label: "Top K", group: "score", step: "1", max: "65535" },
  multiplier: { label: "倍率", group: "score", step: "0.1" },
  priority: { label: "优先级", group: "score", step: "0.1" },
  load: { label: "负载", group: "score", step: "0.1" },
  queue: { label: "队列", group: "score", step: "0.1" },
  errorRate: { label: "错误率", group: "score", step: "0.1" },
  ttft: { label: "首字延迟", group: "score", step: "0.1" },
  quotaHeadroom: { label: "额度余量", group: "score", step: "0.1" },
  previousResponse: { label: "响应连续性", group: "sticky", step: "0.1" },
  sessionSticky: { label: "会话粘性", group: "sticky", step: "0.1" },
  multiplierMinConfidence: { label: "倍率证据阈值", group: "boundary", step: "0.05", max: "1" },
  stickyEscapeTtftMs: { label: "逃逸 TTFT (ms)", group: "sticky", step: "1" },
  stickyEscapeErrorRate: { label: "逃逸错误率", group: "sticky", step: "0.05", max: "1" },
  stickySessionTtlSeconds: { label: "会话 TTL (秒)", group: "sticky", step: "1" },
  stickyResponseTtlSeconds: { label: "响应 TTL (秒)", group: "sticky", step: "1" },
  stickyMaxWaiting: { label: "粘性最大等待", group: "waiting", step: "1" },
  stickyWaitTimeoutSeconds: { label: "粘性等待超时 (秒)", group: "waiting", step: "1" },
  fallbackMaxWaiting: { label: "兜底最大等待", group: "waiting", step: "1" },
  fallbackWaitTimeoutSeconds: { label: "兜底等待超时 (秒)", group: "waiting", step: "1" },
} as const satisfies Record<SchedulerNumericField, SchedulerFieldMeta>;

export const SCHEDULER_BOOLEAN_FIELD_META = {
  stickyWeighted: { label: "加权粘性", group: "sticky" },
  stickyEscape: { label: "粘性逃逸", group: "sticky" },
} as const satisfies Record<SchedulerBooleanField, { label: string; group: SchedulerFieldGroup }>;

export const ROUTING_GROUP_PRESET_OPTIONS: ReadonlyArray<{
  value: Exclude<RoutingGroupPreset, "current_specific">;
  label: string;
}> = [
  { value: "all_groups", label: "全部分组" },
  { value: "gpt", label: "GPT 分组" },
  { value: "claude", label: "Claude 分组" },
  { value: "gemini", label: "Gemini 分组" },
  { value: "grok", label: "Grok 分组" },
  { value: "image_generation", label: "图像生成" },
  { value: "ungrouped_only", label: "仅未分组" },
];

const BASE_SCORE_WEIGHT_FIELDS = [
  "multiplier",
  "priority",
  "load",
  "queue",
  "errorRate",
  "ttft",
  "quotaHeadroom",
] as const satisfies ReadonlyArray<SchedulerNumericField>;

export function createLocalRoutingSettingsDraft(settings: AppSettings): LocalRoutingSettingsDraft {
  const scheduler = Object.fromEntries(
    typedEntries(SCHEDULER_ADVANCED_FIELD_KINDS).map(([key, kind]) => [
      key,
      kind === "boolean"
        ? settings.schedulerAdvancedSettings[key]
        : String(settings.schedulerAdvancedSettings[key]),
    ]),
  ) as SchedulerDraft;

  return {
    maxRateMultiplier:
      settings.maxRateMultiplier == null ? "" : String(settings.maxRateMultiplier),
    defaultRoutingGroupPreset: routingGroupFilterToPreset(settings.defaultRoutingGroupFilter),
    currentRoutingGroupFilter: settings.defaultRoutingGroupFilter,
    scheduler,
  };
}

export function parseLocalRoutingSettingsDraft(
  draft: LocalRoutingSettingsDraft,
): ParsedLocalRoutingSettingsDraft {
  const errors: LocalRoutingSettingsErrors = {};
  const schedulerValues: Partial<Record<keyof SchedulerAdvancedSettings, number | boolean>> = {};
  const maxRateMultiplier = parseNullableNonNegativeNumber(
    draft.maxRateMultiplier,
    "倍率上限必须是大于或等于 0 的数字",
    (message) => {
      errors.maxRateMultiplier = message;
    },
  );

  for (const [key, kind] of typedEntries(SCHEDULER_ADVANCED_FIELD_KINDS)) {
    const value = draft.scheduler[key];
    if (kind === "boolean") {
      schedulerValues[key] = value === true;
      continue;
    }
    const parsed = parseSchedulerNumber(key, kind, String(value), errors);
    if (parsed != null) {
      schedulerValues[key] = parsed;
    }
  }

  if (
    BASE_SCORE_WEIGHT_FIELDS.every(
      (key) => schedulerValues[key] === 0,
    )
  ) {
    errors.baseWeights = "至少保留一个大于 0 的基础评分参数";
  }

  if (Object.keys(errors).length > 0) {
    return { ok: false, errors };
  }

  return {
    ok: true,
    value: {
      maxRateMultiplier,
      defaultRoutingGroupFilter: routingGroupPresetToFilter(
        draft.defaultRoutingGroupPreset,
        draft.currentRoutingGroupFilter,
      ),
      schedulerAdvancedSettings: schedulerValues as SchedulerAdvancedSettings,
    },
  };
}

function parseSchedulerNumber(
  key: SchedulerNumericField,
  kind: Exclude<SchedulerAdvancedFieldKind, "boolean">,
  rawValue: string,
  errors: LocalRoutingSettingsErrors,
) {
  const value = Number(rawValue.trim());
  if (!rawValue.trim() || !Number.isFinite(value)) {
    errors[key] = "请输入有效数字";
    return null;
  }
  if (kind === "positiveInteger") {
    if (!Number.isSafeInteger(value) || value <= 0 || (key === "topK" && value > 65_535)) {
      errors[key] = key === "topK" ? "请输入 1 到 65535 的整数" : "请输入大于 0 的整数";
      return null;
    }
    return value;
  }
  if (kind === "ratio") {
    if (value < 0 || value > 1) {
      errors[key] = "请输入 0 到 1 之间的数字";
      return null;
    }
    return value;
  }
  if (value < 0) {
    errors[key] = "请输入大于或等于 0 的数字";
    return null;
  }
  return value;
}

function parseNullableNonNegativeNumber(
  rawValue: string,
  invalidMessage: string,
  reportError: (message: string) => void,
) {
  const trimmed = rawValue.trim();
  if (!trimmed) {
    return null;
  }
  const value = Number(trimmed);
  if (!Number.isFinite(value) || value < 0) {
    reportError(invalidMessage);
    return null;
  }
  return value;
}

function routingGroupFilterToPreset(filter: RoutingGroupFilter): RoutingGroupPreset {
  if (filter === "all_groups" || filter === "ungrouped_only") {
    return filter;
  }
  if ("group_type" in filter) {
    return filter.group_type;
  }
  return "current_specific";
}

function routingGroupPresetToFilter(
  preset: RoutingGroupPreset,
  currentFilter: RoutingGroupFilter,
): RoutingGroupFilter {
  if (preset === "current_specific") {
    return currentFilter;
  }
  if (preset === "all_groups" || preset === "ungrouped_only") {
    return preset;
  }
  return { group_type: preset };
}

function typedEntries<ObjectType extends object>(value: ObjectType) {
  return Object.entries(value) as Array<{
    [Key in keyof ObjectType]: [Key, ObjectType[Key]];
  }[keyof ObjectType]>;
}
