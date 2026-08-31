export type CollectorProxyMode = "direct" | "system" | "manual";

export type AppSettings = {
  localProxyPort: number;
  localProxyStartOnLaunch: boolean;
  localKeyMasked: string;
  collectorProxyMode: CollectorProxyMode;
  collectorProxyUrl: string | null;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  publishedStatusIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  developerModeEnabled: boolean;
  showDecisionExplanation?: boolean;
  dataDir: string;
  pendingDataDir: string | null;
  dataDirChangeRequiresRestart: boolean;
};

export type CcswitchImportResult = {
  app: string;
  providerName: string;
  endpoint: string;
};

export type CommonLoginEmail = {
    id: string;
    email: string;
};

export type CommonLoginPassword = {
  id: string;
  passwordMasked: string;
};

export type CommonLoginOptions = {
  emails: CommonLoginEmail[];
  passwords: CommonLoginPassword[];
};

export type UpsertCommonLoginEmailInput = {
  id?: string | null;
  email: string;
};

export type UpsertCommonLoginPasswordInput = {
  id?: string | null;
  password: string;
};

export type UpdateSettingsInput = {
  localProxyPort: number;
  collectorProxyMode: CollectorProxyMode;
  collectorProxyUrl: string | null;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  publishedStatusIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  developerModeEnabled: boolean;
  showDecisionExplanation: boolean;
};

export function appSettingsToUpdateInput(settings: AppSettings): UpdateSettingsInput {
  return {
    localProxyPort: settings.localProxyPort,
    collectorProxyMode: settings.collectorProxyMode,
    collectorProxyUrl: settings.collectorProxyUrl,
    lowBalanceThresholdCny: settings.lowBalanceThresholdCny,
    collectorIntervalMinutes: settings.collectorIntervalMinutes,
    balanceIntervalMinutes: settings.balanceIntervalMinutes,
    groupRateIntervalMinutes: settings.groupRateIntervalMinutes,
    publishedStatusIntervalMinutes: settings.publishedStatusIntervalMinutes,
    pricingRefreshIntervalMinutes: settings.pricingRefreshIntervalMinutes,
    collectorTimeoutSeconds: settings.collectorTimeoutSeconds,
    collectorMaxConcurrency: settings.collectorMaxConcurrency,
    developerModeEnabled: settings.developerModeEnabled,
    showDecisionExplanation: settings.showDecisionExplanation ?? false,
  };
}

export const collectorProxyModeLabels: Record<CollectorProxyMode, string> = {
  direct: "直连",
  system: "使用系统代理",
  manual: "手动代理地址",
};
