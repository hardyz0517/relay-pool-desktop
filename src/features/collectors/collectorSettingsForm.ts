import type { AppSettings } from "@/lib/types/settings";

export type CollectorFrequencyPreset =
  | "timely"
  | "balanced"
  | "resource_saver"
  | "custom";

export type CollectorSettingsDraft = {
  balanceIntervalMinutes: string;
  groupRateIntervalMinutes: string;
  publishedStatusIntervalMinutes: string;
  pricingRefreshIntervalMinutes: string;
  collectorTimeoutSeconds: string;
  collectorMaxConcurrency: string;
};

export type CollectorSettingsField = keyof CollectorSettingsDraft;
export type CollectorSettingsErrors = Partial<Record<CollectorSettingsField, string>>;

export type CollectorSettingsValue = {
  [key in CollectorSettingsField]: number;
};

export const COLLECTOR_TIMEOUT_SECONDS = {
  min: 3,
  max: 300,
  recommended: 60,
} as const;

type ParsedCollectorSettingsDraft =
  | { ok: true; value: CollectorSettingsValue }
  | { ok: false; errors: CollectorSettingsErrors };

const FREQUENCY_PRESETS = {
  timely: {
    balanceIntervalMinutes: "2",
    groupRateIntervalMinutes: "10",
    publishedStatusIntervalMinutes: "2",
    pricingRefreshIntervalMinutes: "30",
  },
  balanced: {
    balanceIntervalMinutes: "5",
    groupRateIntervalMinutes: "20",
    publishedStatusIntervalMinutes: "5",
    pricingRefreshIntervalMinutes: "60",
  },
  resource_saver: {
    balanceIntervalMinutes: "15",
    groupRateIntervalMinutes: "60",
    publishedStatusIntervalMinutes: "15",
    pricingRefreshIntervalMinutes: "180",
  },
} as const;

export function createCollectorSettingsDraft(
  settings: Pick<AppSettings, CollectorSettingsField>,
): CollectorSettingsDraft {
  return {
    balanceIntervalMinutes: String(settings.balanceIntervalMinutes),
    groupRateIntervalMinutes: String(settings.groupRateIntervalMinutes),
    publishedStatusIntervalMinutes: String(settings.publishedStatusIntervalMinutes),
    pricingRefreshIntervalMinutes: String(settings.pricingRefreshIntervalMinutes),
    collectorTimeoutSeconds: String(settings.collectorTimeoutSeconds),
    collectorMaxConcurrency: String(settings.collectorMaxConcurrency),
  };
}

export function detectCollectorFrequencyPreset(
  draft: CollectorSettingsDraft,
): CollectorFrequencyPreset {
  for (const [preset, values] of Object.entries(FREQUENCY_PRESETS)) {
    if (
      Object.entries(values).every(
        ([field, value]) => draft[field as keyof typeof values] === value,
      )
    ) {
      return preset as Exclude<CollectorFrequencyPreset, "custom">;
    }
  }
  return "custom";
}

export function applyCollectorFrequencyPreset(
  draft: CollectorSettingsDraft,
  preset: Exclude<CollectorFrequencyPreset, "custom">,
): CollectorSettingsDraft {
  return { ...draft, ...FREQUENCY_PRESETS[preset] };
}

export function createRecommendedCollectorSettingsDraft(): CollectorSettingsDraft {
  return {
    ...FREQUENCY_PRESETS.balanced,
    collectorTimeoutSeconds: String(COLLECTOR_TIMEOUT_SECONDS.recommended),
    collectorMaxConcurrency: "3",
  };
}

export function parseCollectorSettingsDraft(
  draft: CollectorSettingsDraft,
): ParsedCollectorSettingsDraft {
  const errors: CollectorSettingsErrors = {};
  const value = {} as CollectorSettingsValue;
  const intervalFields: CollectorSettingsField[] = [
    "balanceIntervalMinutes",
    "groupRateIntervalMinutes",
    "pricingRefreshIntervalMinutes",
  ];

  for (const field of intervalFields) {
    value[field] = parseInteger(draft[field], 1, Number.MAX_SAFE_INTEGER, errors, field);
  }
  value.publishedStatusIntervalMinutes = parseInteger(
    draft.publishedStatusIntervalMinutes,
    1,
    1440,
    errors,
    "publishedStatusIntervalMinutes",
  );
  value.collectorTimeoutSeconds = parseInteger(
    draft.collectorTimeoutSeconds,
    COLLECTOR_TIMEOUT_SECONDS.min,
    COLLECTOR_TIMEOUT_SECONDS.max,
    errors,
    "collectorTimeoutSeconds",
  );
  value.collectorMaxConcurrency = parseInteger(
    draft.collectorMaxConcurrency,
    1,
    8,
    errors,
    "collectorMaxConcurrency",
  );

  return Object.keys(errors).length > 0
    ? { ok: false, errors }
    : { ok: true, value };
}

function parseInteger(
  rawValue: string,
  min: number,
  max: number,
  errors: CollectorSettingsErrors,
  field: CollectorSettingsField,
) {
  const value = Number(rawValue.trim());
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    errors[field] = max === Number.MAX_SAFE_INTEGER
      ? `请输入大于或等于 ${min} 的整数`
      : `请输入 ${min} 到 ${max} 的整数`;
    return min;
  }
  return value;
}
