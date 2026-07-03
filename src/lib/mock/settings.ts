export type MockSettings = {
  proxyPort: number;
  maskedLocalKey: string;
  collectionIntervalMinutes: number;
  lowBalanceThresholdCny: number;
  dataDir: string;
  trayBehavior: "minimize-to-tray" | "close-to-tray" | "disabled";
  developerModeEnabled: boolean;
  themeNote: string;
};

export const mockSettings: MockSettings = {
  proxyPort: 8787,
  maskedLocalKey: "sk-local-pool-****-2w9",
  collectionIntervalMinutes: 30,
  lowBalanceThresholdCny: 15,
  dataDir: "%APPDATA%\\Relay Pool Desktop",
  trayBehavior: "minimize-to-tray",
  developerModeEnabled: false,
  themeNote: "当前使用浅色桌面工具主题。",
};
