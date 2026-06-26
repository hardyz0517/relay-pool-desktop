export type StationType = "sub2api" | "newapi" | "openai-compatible" | "custom";

export type StationStatus =
  | "healthy"
  | "warning"
  | "error"
  | "disabled"
  | "unchecked";

export type Station = {
  id: string;
  name: string;
  stationType: StationType;
  baseUrl: string;
  apiKeyMasked: string;
  apiKeyPresent: boolean;
  keyCount: number;
  enabled: boolean;
  priority: number;
  creditPerCny: number;
  balanceRaw: number | null;
  balanceCny: number | null;
  lowBalanceThresholdCny: number | null;
  status: StationStatus;
  latencyMs: number | null;
  lastCheckedAt: string | null;
  lastPricingFetchedAt: string | null;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type StationInput = {
  name: string;
  stationType: StationType;
  baseUrl: string;
  apiKey: string;
  enabled: boolean;
  creditPerCny: number;
  lowBalanceThresholdCny: number | null;
  note: string | null;
};

export type StationUpdateInput = Omit<StationInput, "apiKey"> & {
  id: string;
  apiKey: string | null;
};

export const stationTypeLabels: Record<StationType, string> = {
  sub2api: "Sub2API",
  newapi: "NewAPI",
  "openai-compatible": "OpenAI-compatible",
  custom: "Custom",
};

export const stationStatusLabels: Record<StationStatus, string> = {
  healthy: "正常",
  warning: "警告",
  error: "错误",
  disabled: "禁用",
  unchecked: "未检测",
};
