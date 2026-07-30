export type ProviderDraftGroup = {
  clientId: string;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  rateMultiplier: number | null;
  inferredGroupCategory: string | null;
  groupCategoryOverride: string | null;
  source: string;
};

export type ProviderDraftKey = {
  clientId: string;
  name: string;
  enabled: boolean;
  groupClientId: string | null;
  groupIdHash: string | null;
  groupName: string | null;
  rateMultiplier: number | null;
  note: string | null;
};

export type ProviderDraftPayload = {
  name: string;
  stationType: string;
  websiteUrl: string;
  apiBaseUrl: string;
  collectorProxyMode: string;
  collectorProxyUrl: string | null;
  enabled: boolean;
  creditPerCny: number;
  lowBalanceThresholdCny: number | null;
  collectionIntervalMinutes: number;
  note: string | null;
  loginUsername: string | null;
  rememberPassword: boolean;
  groups: ProviderDraftGroup[];
  keys: ProviderDraftKey[];
};

export type ProviderDraft = {
  id: string;
  baseStationId: string | null;
  revision: number;
  state: string;
  payloadSchemaVersion: number;
  payload: ProviderDraftPayload;
  stationApiKeyPresent: boolean;
  loginPasswordPresent: boolean;
  keyApiKeyClientIds: string[];
  committedStationId: string | null;
  createdAt: string;
  updatedAt: string;
  expiresAt: string;
};

export type ProviderDraftPreviewGroup = {
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  rateMultiplier: number | null;
  inferredGroupCategory: string | null;
  source: string;
  confidence: number;
};

export type ProviderDraftPreview = {
  draftId: string;
  kind: string;
  runtimeFingerprint: string;
  status: string;
  groups: ProviderDraftPreviewGroup[];
  models: string[];
  balance: number | null;
  summaryJson: Record<string, unknown>;
  collectedAt: string;
};

export type ProviderDraftPatch = {
  draftId: string;
  expectedRevision: number;
  payload: ProviderDraftPayload;
  stationApiKey: string | null;
  loginPassword: string | null;
  keyApiKeys: Array<{ clientId: string; apiKey: string }>;
};
