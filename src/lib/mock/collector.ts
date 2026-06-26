export type MockCollectorSource = "frontend-api" | "webview-capture" | "html" | "manual";

export type MockCollectorSnapshot = {
  stationId: string;
  stationName: string;
  loginStatus: "logged-in" | "expired" | "unknown";
  source: MockCollectorSource;
  fetchedAt: string;
  capturedEndpoints: string[];
  detectedBalanceField: string;
  detectedGroupFields: string[];
  detectedRateFields: string[];
  snapshotSummary: string[];
  failureReason?: string;
};

export const collectorSourceLabels: Record<MockCollectorSource, string> = {
  "frontend-api": "frontend-api",
  "webview-capture": "webview-capture",
  html: "html",
  manual: "manual",
};

export const mockCollectorSnapshot: MockCollectorSnapshot = {
  stationId: "st-orchid",
  stationName: "Orchid Relay",
  loginStatus: "logged-in",
  source: "frontend-api",
  fetchedAt: "今天 08:45",
  capturedEndpoints: [
    "GET /api/user/self",
    "GET /api/token/?p=1",
    "GET /api/channel/models",
    "GET /api/ratio_config",
  ],
  detectedBalanceField: "data.quota",
  detectedGroupFields: ["data.group", "data.group_name"],
  detectedRateFields: ["model_ratio.gpt-4.1", "completion_ratio", "group_ratio.default"],
  snapshotSummary: ["识别 3 个分组", "识别 18 个模型倍率", "余额字段可信度 92%"],
};

export const mockCollectorFailure: MockCollectorSnapshot = {
  stationId: "st-harbor",
  stationName: "Harbor Compatible",
  loginStatus: "expired",
  source: "html",
  fetchedAt: "今天 08:50",
  capturedEndpoints: ["GET /dashboard", "GET /assets/app.js"],
  detectedBalanceField: "未识别",
  detectedGroupFields: [],
  detectedRateFields: [],
  snapshotSummary: ["页面结构变化", "未发现倍率 JSON", "建议手动校正"],
  failureReason: "登录态过期，HTML 兜底解析未识别 rate_multiplier 字段。",
};
