import { mockPricingRows } from "./pricing";
import { mockRequestLogs } from "./logs";
import { mockStations } from "./stations";

export const mockDashboard = {
  proxyRunning: false,
  baseUrl: "http://127.0.0.1:8787/v1",
  maskedLocalKey: "sk-local-pool-****-2w9",
  strategy: "手动排序优先",
  enabledStationCount: mockStations.filter((station) => station.enabled).length,
  balanceAlertCount: mockStations.filter(
    (station) => station.enabled && station.balanceCny < 15,
  ).length,
  todayRequests: 128,
  todayCostCny: 6.72,
  recentRequests: mockRequestLogs,
  priceChanges: mockPricingRows.map((row) => ({
    model: row.model,
    stationName: row.recommendedStationName,
    deltaPercent: row.deltaPercent,
    updatedAt: row.updatedAt,
  })),
  healthSummary: {
    healthy: mockStations.filter((station) => station.status === "healthy").length,
    warning: mockStations.filter((station) => station.status === "warning").length,
    error: mockStations.filter((station) => station.status === "error").length,
    disabled: mockStations.filter((station) => station.status === "disabled").length,
  },
};
