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
  accessTokenPresent: boolean;
  refreshTokenPresent: boolean;
  cookiePresent: boolean;
  sessionSource: string | null;
  newapiUserId: string | null;
  tokenExpiresAt: string | null;
  tokenRefreshedAt: string | null;
  updatedAt: string | null;
};

export type UpdateStationSessionInput = {
  stationId: string;
  accessToken: string | null;
  refreshToken: string | null;
  cookie: string | null;
  newapiUserId: string | null;
  tokenExpiresAt: string | null;
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

export type SaveStationKeyMode = "create" | "update";

export type StationKeyGroupSelection =
  | { kind: "keep" }
  | { kind: "clear" }
  | {
      kind: "set";
      groupBindingId: string;
      groupIdHash?: string | null;
      groupName?: string | null;
    };

export type SaveStationKeyWithDefaultsInput = {
  mode: SaveStationKeyMode;
  id?: string | null;
  stationId: string;
  name: string;
  apiKey?: string | null;
  enabled: boolean;
  priority?: number | null;
  tierLabel?: string | null;
  balanceScope?: string | null;
  status?: StationKeyStatus | null;
  note?: string | null;
  groupSelection: StationKeyGroupSelection;
};

export type SaveStationKeyWithDefaultsResult = {
  stationKey: StationKey;
  capabilities: import("@/lib/types/routing").StationKeyCapabilities;
  message: string;
};

export type StationKeyConnectivityTestResult = {
  stationKeyId: string;
  ok: boolean;
  statusCode: number;
  durationMs: number;
  model: string;
  message: string;
};

export type RemoteKeyMatchStatus = "matched" | "possible" | "unbound";

export type RemoteKeyCapability = {
  stationId: string;
  stationType: string;
  canListRemoteKeys: boolean;
  canCreateRemoteKey: boolean;
  canReadGroups: boolean;
  requiresManualSession: boolean;
  unsupportedReason: string | null;
};

export type RemoteStationKey = {
  id: string;
  stationId: string;
  remoteKeyIdHash: string | null;
  remoteKeyName: string | null;
  apiKeyMasked: string | null;
  apiKeyFingerprint: string | null;
  groupIdHash: string | null;
  groupName: string | null;
  tierLabel: string | null;
  rateMultiplier: number | null;
  rateSource: string | null;
  createdAt: string | null;
  lastUsedAt: string | null;
  rawSource: string;
  matchStatus: RemoteKeyMatchStatus;
  matchedStationKeyId: string | null;
  matchConfidence: number;
  collectedAt: string;
};

export type RemoteKeyScanResult = {
  stationId: string;
  capability: RemoteKeyCapability;
  keys: RemoteStationKey[];
  syncedStationKeyIds: string[];
  message: string;
};

export type CreateRemoteStationKeyInput = {
  stationId: string;
  name: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupName: string | null;
};

export type CreateRemoteStationKeyResult = {
  remoteKey: RemoteStationKey;
  stationKey: StationKey;
  fullKeyOnce: string | null;
  message: string;
};

export type CreateLocalStationKeyFromRemoteResult = {
  remoteKey: RemoteStationKey;
  stationKey: StationKey;
  message: string;
};

export const stationKeyStatusLabels: Record<StationKeyStatus, string> = {
  unchecked: "未检测",
  healthy: "正常",
  warning: "警告",
  error: "错误",
  disabled: "禁用",
};
