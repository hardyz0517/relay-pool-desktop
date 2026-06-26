import { mockStations } from "./stations";

export type MockChannelStatus = "healthy" | "warning" | "error" | "disabled" | "unchecked";

export type MockChannelHealth = {
  stationId: string;
  stationName: string;
  stationType: string;
  modelSummary: string;
  status: MockChannelStatus;
  latencyMs: number | null;
  pingMs: number | null;
  availabilityPercent: number;
  recentOutcomes: Array<"success" | "warning" | "failed" | "unknown">;
  lastCheckedAt: string;
  lastError: string;
};

const statusByStationId: Record<string, MockChannelStatus> = {
  "st-orchid": "healthy",
  "st-lantern": "warning",
  "st-harbor": "error",
  "st-archive": "disabled",
};

const modelSummaryByStationId: Record<string, string> = {
  "st-orchid": "gpt-4.1 / claude-sonnet-4",
  "st-lantern": "gpt-4.1-mini / gemini-2.5-pro",
  "st-harbor": "gpt-4o-mini / deepseek-chat",
  "st-archive": "legacy-model",
};

const errorByStationId: Record<string, string> = {
  "st-orchid": "最近一次检测正常。",
  "st-lantern": "余额接近阈值，建议关注。",
  "st-harbor": "最近一次健康检测返回 429 rate_limit。",
  "st-archive": "用户手动禁用。",
};

const availabilityByStationId: Record<string, number> = {
  "st-orchid": 99.2,
  "st-lantern": 93.4,
  "st-harbor": 81.7,
  "st-archive": 0,
};

const recentOutcomesByStationId: Record<string, Array<"success" | "warning" | "failed" | "unknown">> = {
  "st-orchid": Array.from({ length: 60 }, (_, index) => (index % 12 === 0 ? "warning" : "success")),
  "st-lantern": Array.from({ length: 60 }, (_, index) => (index % 14 === 0 ? "warning" : index % 9 === 0 ? "failed" : "success")),
  "st-harbor": Array.from({ length: 60 }, (_, index) => (index % 6 === 0 ? "failed" : index % 4 === 0 ? "warning" : "success")),
  "st-archive": Array.from({ length: 60 }, () => "unknown"),
};

export const mockChannelHealths: MockChannelHealth[] = mockStations.map((station) => ({
  stationId: station.id,
  stationName: station.name,
  stationType: station.type,
  modelSummary: modelSummaryByStationId[station.id] ?? "未知",
  status: statusByStationId[station.id] ?? "unchecked",
  latencyMs: station.latencyMs,
  pingMs: station.latencyMs ? Math.max(18, Math.round(station.latencyMs * 0.42)) : null,
  availabilityPercent: availabilityByStationId[station.id] ?? 0,
  recentOutcomes: recentOutcomesByStationId[station.id] ?? Array.from({ length: 60 }, () => "unknown"),
  lastCheckedAt: station.lastCheckedAt ?? "未检测",
  lastError: errorByStationId[station.id] ?? "暂无错误。",
}));
