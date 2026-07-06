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
  collectionIntervalMinutes: number;
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
  collectionIntervalMinutes: number;
  note: string | null;
};

export type StationUpdateInput = Omit<StationInput, "apiKey"> & {
  id: string;
  apiKey: string | null;
};

export type StationEndpointHealth = {
  stationId: string;
  status: "unchecked" | "success" | "failed";
  latencyMs: number | null;
  checkedAt: string | null;
  errorSummary: string | null;
  updatedAt: string;
};

export type EndpointPingResult = {
  stationId: string;
  ok: boolean;
  status: "success" | "failed";
  latencyMs: number | null;
  checkedAt: string;
  errorSummary: string | null;
};

export const stationTypeLabels: Record<StationType, string> = {
  sub2api: "Sub2API",
  newapi: "NewAPI",
  "openai-compatible": "自定义接口",
  custom: "自定义接口",
};

export const stationTypeOptions: Array<{ value: StationType; label: string }> = [
  { value: "sub2api", label: stationTypeLabels.sub2api },
  { value: "newapi", label: stationTypeLabels.newapi },
  { value: "custom", label: stationTypeLabels.custom },
];

export const stationStatusLabels: Record<StationStatus, string> = {
  healthy: "采集正常",
  warning: "采集需关注",
  error: "采集异常",
  disabled: "禁用",
  unchecked: "未采集",
};
