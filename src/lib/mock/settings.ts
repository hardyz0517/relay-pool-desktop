export type MockSettings = {
  proxyPort: number;
  maskedLocalKey: string;
  collectionIntervalMinutes: number;
  lowBalanceThresholdCny: number;
  dataDir: string;
  trayBehavior: "minimize-to-tray" | "close-to-tray" | "disabled";
  themeNote: string;
};

export const mockSettings: MockSettings = {
  proxyPort: 8787,
  maskedLocalKey: "sk-local-pool-****-2w9",
  collectionIntervalMinutes: 30,
  lowBalanceThresholdCny: 15,
  dataDir: "%APPDATA%\\Relay Pool Desktop",
  trayBehavior: "minimize-to-tray",
  themeNote: "第一版默认浅色主题；深色主题仅作为后续可选项预留。",
};
