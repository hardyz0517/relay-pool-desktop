export type RoutingStrategy = "manual" | "cheapest" | "stable";

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
  manual: "手动排序优先",
  cheapest: "最低价优先",
  stable: "稳定性优先",
};

export const trayBehaviorLabels: Record<TrayBehavior, string> = {
  "minimize-to-tray": "最小化到托盘",
  "close-to-tray": "关闭到托盘",
  disabled: "禁用",
};
