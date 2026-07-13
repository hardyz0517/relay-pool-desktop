import type { RoutingGroupFilter, SchedulerAdvancedSettings } from "@/lib/types/routing";

export type RoutingStrategy =
  | "automatic_balanced"
  | "priority_fallback"
  | "stable_first"
  | "backup_only"
  | "cheap_first"
  | "cost_stable_first";

export type CollectorProxyMode = "direct" | "system" | "manual";

export type SchedulerAdvancedFieldKind =
  | "positiveInteger"
  | "nonNegativeWeight"
  | "ratio"
  | "boolean";

export const SCHEDULER_ADVANCED_FIELD_KINDS = {
  topK: "positiveInteger",
  multiplier: "nonNegativeWeight",
  priority: "nonNegativeWeight",
  load: "nonNegativeWeight",
  queue: "nonNegativeWeight",
  errorRate: "nonNegativeWeight",
  ttft: "nonNegativeWeight",
  quotaHeadroom: "nonNegativeWeight",
  previousResponse: "nonNegativeWeight",
  sessionSticky: "nonNegativeWeight",
  multiplierMinConfidence: "ratio",
  stickyWeighted: "boolean",
  stickyEscape: "boolean",
  stickyEscapeTtftMs: "positiveInteger",
  stickyEscapeErrorRate: "ratio",
  stickySessionTtlSeconds: "positiveInteger",
  stickyResponseTtlSeconds: "positiveInteger",
  stickyMaxWaiting: "positiveInteger",
  stickyWaitTimeoutSeconds: "positiveInteger",
  fallbackMaxWaiting: "positiveInteger",
  fallbackWaitTimeoutSeconds: "positiveInteger",
} as const satisfies Record<keyof SchedulerAdvancedSettings, SchedulerAdvancedFieldKind>;

export const DEFAULT_SCHEDULER_ADVANCED_SETTINGS: SchedulerAdvancedSettings = {
  topK: 7,
  multiplier: 1.0,
  priority: 1.0,
  load: 1.0,
  queue: 0.7,
  errorRate: 0.8,
  ttft: 0.5,
  quotaHeadroom: 0.0,
  previousResponse: 5.0,
  sessionSticky: 3.0,
  multiplierMinConfidence: 0.8,
  stickyWeighted: false,
  stickyEscape: true,
  stickyEscapeTtftMs: 15_000,
  stickyEscapeErrorRate: 0.5,
  stickySessionTtlSeconds: 3_600,
  stickyResponseTtlSeconds: 3_600,
  stickyMaxWaiting: 3,
  stickyWaitTimeoutSeconds: 120,
  fallbackMaxWaiting: 100,
  fallbackWaitTimeoutSeconds: 30,
};

export type AppSettings = {
  localProxyPort: number;
  localKeyMasked: string;
  defaultRoutingStrategy: RoutingStrategy;
  collectorProxyMode: CollectorProxyMode;
  collectorProxyUrl: string | null;
  maxRateMultiplier: number | null;
  defaultRoutingGroupFilter: RoutingGroupFilter;
  schedulerAdvancedSettings: SchedulerAdvancedSettings;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  modelListIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  allowDepletedFallback: boolean;
  developerModeEnabled: boolean;
  dataDir: string;
  pendingDataDir: string | null;
  dataDirChangeRequiresRestart: boolean;
};

export type CcswitchImportResult = {
  app: string;
  providerName: string;
  endpoint: string;
};

export type UpdateSettingsInput = {
  localProxyPort: number;
  defaultRoutingStrategy: RoutingStrategy;
  collectorProxyMode: CollectorProxyMode;
  collectorProxyUrl: string | null;
  maxRateMultiplier: number | null;
  defaultRoutingGroupFilter: RoutingGroupFilter;
  schedulerAdvancedSettings: SchedulerAdvancedSettings;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  modelListIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  allowDepletedFallback: boolean;
  developerModeEnabled: boolean;
};

export function appSettingsToUpdateInput(settings: AppSettings): UpdateSettingsInput {
  return {
    localProxyPort: settings.localProxyPort,
    defaultRoutingStrategy: settings.defaultRoutingStrategy,
    collectorProxyMode: settings.collectorProxyMode,
    collectorProxyUrl: settings.collectorProxyUrl,
    maxRateMultiplier: settings.maxRateMultiplier,
    defaultRoutingGroupFilter: settings.defaultRoutingGroupFilter,
    schedulerAdvancedSettings: settings.schedulerAdvancedSettings,
    lowBalanceThresholdCny: settings.lowBalanceThresholdCny,
    collectorIntervalMinutes: settings.collectorIntervalMinutes,
    balanceIntervalMinutes: settings.balanceIntervalMinutes,
    groupRateIntervalMinutes: settings.groupRateIntervalMinutes,
    modelListIntervalMinutes: settings.modelListIntervalMinutes,
    pricingRefreshIntervalMinutes: settings.pricingRefreshIntervalMinutes,
    collectorTimeoutSeconds: settings.collectorTimeoutSeconds,
    collectorMaxConcurrency: settings.collectorMaxConcurrency,
    allowDepletedFallback: settings.allowDepletedFallback,
    developerModeEnabled: settings.developerModeEnabled,
  };
}

export const routingStrategyLabels: Record<RoutingStrategy, string> = {
  automatic_balanced: "自动路由",
  priority_fallback: "优先级轮换",
  stable_first: "稳定优先",
  backup_only: "备用模式",
  cheap_first: "便宜优先",
  cost_stable_first: "低价稳定优先",
};

export const collectorProxyModeLabels: Record<CollectorProxyMode, string> = {
  direct: "直连",
  system: "使用系统代理",
  manual: "手动代理地址",
};
