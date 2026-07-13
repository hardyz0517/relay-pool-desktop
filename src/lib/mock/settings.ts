export type MockSettings = {
  proxyPort: number;
  maskedLocalKey: string;
  collectionIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  modelListIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  allowDepletedFallback: boolean;
  lowBalanceThresholdCny: number;
  dataDir: string;
  pendingDataDir: string | null;
  dataDirChangeRequiresRestart: boolean;
  developerModeEnabled: boolean;
  themeNote: string;
};

export const mockSettings: MockSettings = {
  proxyPort: 8787,
  maskedLocalKey: "sk-local-pool-****-2w9",
  collectionIntervalMinutes: 30,
  balanceIntervalMinutes: 5,
  groupRateIntervalMinutes: 20,
  modelListIntervalMinutes: 60,
  pricingRefreshIntervalMinutes: 60,
  collectorTimeoutSeconds: 15,
  collectorMaxConcurrency: 3,
  allowDepletedFallback: false,
  lowBalanceThresholdCny: 15,
  dataDir: "%APPDATA%\\Relay Pool Desktop",
  pendingDataDir: null,
  dataDirChangeRequiresRestart: false,
  developerModeEnabled: false,
  themeNote: "当前使用浅色桌面工具主题。",
};
