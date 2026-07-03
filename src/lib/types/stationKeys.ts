export type StationKeyStatus = "unchecked" | "healthy" | "warning" | "error" | "disabled";

export type StationKey = {
  id: string;
  stationId: string;
  name: string;
  apiKeyMasked: string;
  apiKeyPresent: boolean;
  enabled: boolean;
  priority: number;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupName: string | null;
  tierLabel: string | null;
  rateMultiplier: number | null;
  rateSource: string | null;
  rateCollectedAt: string | null;
  balanceScope: string | null;
  status: StationKeyStatus;
  lastCheckedAt: string | null;
  lastUsedAt: string | null;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type KeyPoolItem = StationKey & {
  stationName: string;
  stationType: string;
  stationBaseUrl: string;
  capabilitySummary: string[];
  modelScopeSummary: string;
  onlyUseAsBackup: boolean;
  cooldownUntil: string | null;
  successRate: number | null;
  avgLatencyMs: number | null;
  consecutiveFailures: number;
  lastErrorSummary: string | null;
  bindingStatus?: string | null;
  priceState?: string | null;
};

export type StationCredentials = {
  stationId: string;
  loginUsername: string | null;
  passwordPresent: boolean;
  rememberPassword: boolean;
  loginStatus: string;
  loginError: string | null;
  lastLoginAt: string | null;
  sessionStatus: string;
  sessionExpiresAt: string | null;
  updatedAt: string | null;
};

export type CreateStationKeyInput = {
  stationId: string;
  name: string;
  apiKey: string;
  enabled: boolean;
  priority?: number | null;
  groupBindingId?: string | null;
  groupIdHash?: string | null;
  groupName: string | null;
  tierLabel: string | null;
  rateMultiplier?: number | null;
  rateSource?: string | null;
  balanceScope?: string | null;
  note: string | null;
};

export type UpdateStationKeyInput = {
  id: string;
  stationId: string;
  name: string;
  apiKey: string | null;
  enabled: boolean;
  priority: number;
  groupBindingId?: string | null;
  groupIdHash?: string | null;
  groupName: string | null;
  tierLabel: string | null;
  rateMultiplier?: number | null;
  rateSource?: string | null;
  balanceScope?: string | null;
  status: StationKeyStatus;
  note: string | null;
};

export const stationKeyStatusLabels: Record<StationKeyStatus, string> = {
  unchecked: "未检测",
  healthy: "正常",
  warning: "警告",
  error: "错误",
  disabled: "禁用",
};
