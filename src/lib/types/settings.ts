import type { RoutingGroupFilter, SchedulerAdvancedSettings } from "@/lib/types/routing";

export type RoutingStrategy =
  | "automatic_balanced"
  | "priority_fallback"
  | "stable_first"
  | "backup_only"
  | "cheap_first"
  | "cost_stable_first";

export type TrayBehavior = "minimize-to-tray" | "close-to-tray" | "disabled";

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
  trayBehavior: TrayBehavior;
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
  trayBehavior: TrayBehavior;
  developerModeEnabled: boolean;
};

export const routingStrategyLabels: Record<RoutingStrategy, string> = {
  automatic_balanced: "自动路由",
  priority_fallback: "优先级轮换",
  stable_first: "稳定优先",
  backup_only: "备用模式",
  cheap_first: "便宜优先",
  cost_stable_first: "低价稳定优先",
};

export const trayBehaviorLabels: Record<TrayBehavior, string> = {
  "minimize-to-tray": "最小化到托盘",
  "close-to-tray": "关闭到托盘",
  disabled: "禁用",
};
