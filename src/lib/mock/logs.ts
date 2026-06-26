export type MockRequestStatus = "success" | "failed" | "fallback";

export type MockFallbackStep = {
  stationName: string;
  result: "failed" | "selected" | "skipped";
  reason: string;
};

export type MockRequestLog = {
  id: string;
  createdAt: string;
  model: string;
  canonicalModel: string;
  upstreamModel: string;
  stationName: string;
  status: MockRequestStatus;
  fallback: boolean;
  latencyMs: number;
  inputTokens: number;
  outputTokens: number;
  estimatedCostCny: number;
  errorReason?: string;
  candidateStations: string[];
  fallbackTrace: MockFallbackStep[];
  redactedRequestSummary: string;
};

export const requestStatusLabels: Record<MockRequestStatus, string> = {
  success: "成功",
  failed: "失败",
  fallback: "已切换",
};

export const mockRequestLogs: MockRequestLog[] = [
  {
    id: "req-1007",
    createdAt: "09:18:32",
    model: "gpt-4.1",
    canonicalModel: "gpt-4.1",
    upstreamModel: "gpt-4.1-2025-04-14",
    stationName: "Orchid Relay",
    status: "success",
    fallback: false,
    latencyMs: 1840,
    inputTokens: 3820,
    outputTokens: 640,
    estimatedCostCny: 0.043,
    candidateStations: ["Orchid Relay", "Lantern NewAPI"],
    fallbackTrace: [{ stationName: "Orchid Relay", result: "selected", reason: "优先级最高且健康状态正常" }],
    redactedRequestSummary: "POST /v1/responses · messages[3] · key sk-local-****2w9",
  },
  {
    id: "req-1006",
    createdAt: "09:11:04",
    model: "gpt-4.1-mini",
    canonicalModel: "gpt-4.1-mini",
    upstreamModel: "gpt-4.1-mini",
    stationName: "Lantern NewAPI",
    status: "fallback",
    fallback: true,
    latencyMs: 2310,
    inputTokens: 1240,
    outputTokens: 418,
    estimatedCostCny: 0.008,
    errorReason: "Harbor Compatible 返回 429",
    candidateStations: ["Harbor Compatible", "Lantern NewAPI", "Orchid Relay"],
    fallbackTrace: [
      { stationName: "Harbor Compatible", result: "failed", reason: "429 rate_limit" },
      { stationName: "Lantern NewAPI", result: "selected", reason: "fallback 后第一个可用站点" },
    ],
    redactedRequestSummary: "POST /v1/chat/completions · stream=true · key sk-local-****2w9",
  },
  {
    id: "req-1005",
    createdAt: "08:58:19",
    model: "gemini-2.5-pro",
    canonicalModel: "gemini-2.5-pro",
    upstreamModel: "gemini-2.5-pro",
    stationName: "Lantern NewAPI",
    status: "failed",
    fallback: false,
    latencyMs: 920,
    inputTokens: 0,
    outputTokens: 0,
    estimatedCostCny: 0,
    errorReason: "模型不在当前分组",
    candidateStations: ["Lantern NewAPI"],
    fallbackTrace: [{ stationName: "Lantern NewAPI", result: "failed", reason: "403 forbidden" }],
    redactedRequestSummary: "POST /v1/responses · input[1] · key sk-local-****2w9",
  },
];
