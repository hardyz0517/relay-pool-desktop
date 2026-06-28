export type RoutingStrategy = "priority_fallback" | "stable_first" | "backup_only";

export type TrayBehavior = "minimize-to-tray" | "close-to-tray" | "disabled";

export type AppSettings = {
  localProxyPort: number;
  localKeyMasked: string;
  defaultRoutingStrategy: RoutingStrategy;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  trayBehavior: TrayBehavior;
  dataDir: string;
};

export type UpdateSettingsInput = {
  localProxyPort: number;
  defaultRoutingStrategy: RoutingStrategy;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  trayBehavior: TrayBehavior;
};

export const routingStrategyLabels: Record<RoutingStrategy, string> = {
  priority_fallback: "优先级 fallback",
  stable_first: "稳定优先",
  backup_only: "备用模式",
};

export const trayBehaviorLabels: Record<TrayBehavior, string> = {
  "minimize-to-tray": "最小化到托盘",
  "close-to-tray": "关闭到托盘",
  disabled: "禁用",
};
