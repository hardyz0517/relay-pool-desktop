export type MockPricingStatus = "fresh" | "stale" | "unavailable";

export type MockStationPrice = {
  stationName: string;
  inputCnyPer1M: number;
  outputCnyPer1M: number;
  modelRatio: string;
  groupRatio: string;
  health: "正常" | "警告" | "错误";
};

export type MockPricingRow = {
  model: string;
  recommendedStationId: string;
  recommendedStationName: string;
  inputCnyPer1M: number;
  outputCnyPer1M: number;
  stationCount: number;
  updatedAt: string;
  deltaPercent: number;
  status: MockPricingStatus;
  stationPrices: MockStationPrice[];
  recommendReasons: string[];
};

export const pricingStatusLabels: Record<MockPricingStatus, string> = {
  fresh: "已更新",
  stale: "待刷新",
  unavailable: "不可用",
};

export const mockPricingRows: MockPricingRow[] = [
  {
    model: "gpt-4.1",
    recommendedStationId: "st-orchid",
    recommendedStationName: "Orchid Relay",
    inputCnyPer1M: 2.18,
    outputCnyPer1M: 8.72,
    stationCount: 2,
    updatedAt: "今天 08:45",
    deltaPercent: -6.4,
    status: "fresh",
    stationPrices: [
      { stationName: "Orchid Relay", inputCnyPer1M: 2.18, outputCnyPer1M: 8.72, modelRatio: "1.00", groupRatio: "0.82", health: "正常" },
      { stationName: "Lantern NewAPI", inputCnyPer1M: 2.62, outputCnyPer1M: 9.38, modelRatio: "1.00", groupRatio: "0.95", health: "警告" },
    ],
    recommendReasons: ["最低输出价", "余额充足", "健康状态正常"],
  },
  {
    model: "gpt-4.1-mini",
    recommendedStationId: "st-lantern",
    recommendedStationName: "Lantern NewAPI",
    inputCnyPer1M: 0.18,
    outputCnyPer1M: 0.72,
    stationCount: 3,
    updatedAt: "今天 08:45",
    deltaPercent: 1.8,
    status: "fresh",
    stationPrices: [
      { stationName: "Lantern NewAPI", inputCnyPer1M: 0.18, outputCnyPer1M: 0.72, modelRatio: "0.12", groupRatio: "0.80", health: "警告" },
      { stationName: "Orchid Relay", inputCnyPer1M: 0.22, outputCnyPer1M: 0.78, modelRatio: "0.12", groupRatio: "0.92", health: "正常" },
      { stationName: "Harbor Compatible", inputCnyPer1M: 0.2, outputCnyPer1M: 0.86, modelRatio: "0.12", groupRatio: "0.88", health: "错误" },
    ],
    recommendReasons: ["最低输入价", "可用站点最多"],
  },
  {
    model: "claude-sonnet-4",
    recommendedStationId: "st-orchid",
    recommendedStationName: "Orchid Relay",
    inputCnyPer1M: 3.9,
    outputCnyPer1M: 15.6,
    stationCount: 1,
    updatedAt: "昨天 23:10",
    deltaPercent: 0,
    status: "stale",
    stationPrices: [
      { stationName: "Orchid Relay", inputCnyPer1M: 3.9, outputCnyPer1M: 15.6, modelRatio: "1.20", groupRatio: "0.90", health: "正常" },
    ],
    recommendReasons: ["唯一可用站点", "健康状态正常"],
  },
];
