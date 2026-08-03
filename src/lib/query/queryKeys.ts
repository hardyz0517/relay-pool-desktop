import type { DashboardRequestMetricsInput } from "@/lib/types/dashboardMetrics";

export const queryKeys = {
  settings: ["settings"] as const,
  proxyStatus: ["proxyStatus"] as const,
  requestLogs: ["requestLogs"] as const,
  dashboardLiveRequestMetrics: (input: DashboardRequestMetricsInput) =>
    [
      "dashboardRequestMetrics",
      "live",
      1,
      input.localDayStartMs,
      input.localDayEndMs,
    ] as const,
  dashboardCumulativeRequestMetrics: ["dashboardRequestMetrics", "cumulative", 1] as const,
  stations: ["stations"] as const,
  stationAssets: ["stationAssets"] as const,
  stationAssetsForStations: (stationIds: readonly string[]) =>
    ["stationAssets", "stations", stationIds] as const,
  stationAsset: (stationId: string) => ["stationAssets", stationId] as const,
  collectorSnapshots: (stationId: string) => ["collectorSnapshots", stationId] as const,
  collectorRuns: (stationId: string) => ["collectorRuns", stationId] as const,
  captureSessionStatus: (stationId: string) => ["captureSessionStatus", stationId] as const,
  keyPool: ["keyPool"] as const,
  modelBasePrices: ["modelBasePrices"] as const,
  balanceSnapshots: ["balanceSnapshots"] as const,
  changeEvents: ["changeEvents"] as const,
  localRoutingWorkspace: ["localRoutingWorkspace"] as const,
  channelMonitoring: ["channelMonitoring"] as const,
  pricing: ["pricing"] as const,
  channelStatus: ["channelStatus"] as const,
  channelMonitorExecutions: ["channelMonitorExecutions"] as const,
  channelMonitorExecution: (executionId: string) => ["channelMonitorExecution", executionId] as const,
  channelMonitorAttempts: ["channelMonitorAttempts"] as const,
  monitoringCapabilities: ["monitoringCapabilities"] as const,
} as const;
