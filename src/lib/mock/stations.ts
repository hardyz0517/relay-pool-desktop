export type MockStationType =
  | "sub2api"
  | "newapi"
  | "openai-compatible"
  | "custom";

export type MockStationStatus = "healthy" | "warning" | "error" | "disabled";

export type MockStation = {
  id: string;
  name: string;
  type: MockStationType;
  baseUrlHost: string;
  enabled: boolean;
  status: MockStationStatus;
  balanceCny: number;
  latencyMs: number;
  lastCheckedAt: string;
  lastPricingFetchedAt: string;
  supportedModels: string[];
  recentError?: string;
  collectorSource: "frontend-api" | "webview-capture" | "html" | "manual";
};

export const stationTypeLabels: Record<MockStationType, string> = {
  sub2api: "Sub2API",
  newapi: "NewAPI",
  "openai-compatible": "OpenAI-compatible",
  custom: "Custom",
};

export const stationStatusLabels: Record<MockStationStatus, string> = {
  healthy: "正常",
  warning: "警告",
  error: "错误",
  disabled: "禁用",
};

export const mockStations: MockStation[] = [
  {
    id: "st-orchid",
    name: "Orchid Relay",
    type: "sub2api",
    baseUrlHost: "api.orchid-relay.example",
    enabled: true,
    status: "healthy",
    balanceCny: 86.42,
    latencyMs: 428,
    lastCheckedAt: "今天 09:12",
    lastPricingFetchedAt: "今天 08:45",
    supportedModels: ["gpt-4.1", "gpt-4.1-mini", "claude-sonnet-4"],
    collectorSource: "frontend-api",
  },
  {
    id: "st-lantern",
    name: "Lantern NewAPI",
    type: "newapi",
    baseUrlHost: "newapi.lantern.example",
    enabled: true,
    status: "warning",
    balanceCny: 12.35,
    latencyMs: 692,
    lastCheckedAt: "今天 09:08",
    lastPricingFetchedAt: "昨天 23:10",
    supportedModels: ["gpt-4.1-mini", "gemini-2.5-pro"],
    recentError: "余额接近阈值，建议充值或降低优先级。",
    collectorSource: "manual",
  },
  {
    id: "st-harbor",
    name: "Harbor Compatible",
    type: "openai-compatible",
    baseUrlHost: "relay.harbor.example",
    enabled: true,
    status: "error",
    balanceCny: 34.91,
    latencyMs: 0,
    lastCheckedAt: "今天 08:50",
    lastPricingFetchedAt: "今天 08:20",
    supportedModels: ["gpt-4o-mini", "deepseek-chat"],
    recentError: "最近一次健康检测返回 429 rate_limit。",
    collectorSource: "html",
  },
  {
    id: "st-archive",
    name: "Archive Custom",
    type: "custom",
    baseUrlHost: "custom.archive.example",
    enabled: false,
    status: "disabled",
    balanceCny: 0,
    latencyMs: 0,
    lastCheckedAt: "未检测",
    lastPricingFetchedAt: "未采集",
    supportedModels: ["legacy-model"],
    recentError: "用户手动禁用。",
    collectorSource: "manual",
  },
];
