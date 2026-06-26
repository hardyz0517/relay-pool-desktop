export type MockRoutingSettings = {
  defaultStrategy: "manual" | "cheapest" | "stable";
  fallbackEnabled: boolean;
  lowBalanceThresholdCny: number;
  circuitBreakerMinutes: number;
  healthCacheSeconds: number;
  modelOverrides: Array<{
    model: string;
    stationName: string;
    reason: string;
  }>;
};

export const routeStrategyLabels: Record<MockRoutingSettings["defaultStrategy"], string> = {
  manual: "手动排序优先",
  cheapest: "最低价优先",
  stable: "稳定性优先",
};

export const mockRoutingSettings: MockRoutingSettings = {
  defaultStrategy: "manual",
  fallbackEnabled: true,
  lowBalanceThresholdCny: 15,
  circuitBreakerMinutes: 5,
  healthCacheSeconds: 90,
  modelOverrides: [
    { model: "claude-sonnet-4", stationName: "Orchid Relay", reason: "唯一稳定可用" },
    { model: "gemini-2.5-pro", stationName: "Lantern NewAPI", reason: "分组价格最低" },
  ],
};
