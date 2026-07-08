export type RoutingStrategy = "priority_fallback" | "stable_first" | "backup_only" | "cheap_first";

export type TrayBehavior = "minimize-to-tray" | "close-to-tray" | "disabled";

export type AppSettings = {
  localProxyPort: number;
  localKeyMasked: string;
  defaultRoutingStrategy: RoutingStrategy;
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
  priority_fallback: "优先级轮换",
  stable_first: "稳定优先",
  backup_only: "备用模式",
  cheap_first: "便宜优先",
};

export const trayBehaviorLabels: Record<TrayBehavior, string> = {
  "minimize-to-tray": "最小化到托盘",
  "close-to-tray": "关闭到托盘",
  disabled: "禁用",
};
