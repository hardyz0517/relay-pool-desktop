import type {
  DashboardCostMetricsDto,
  DashboardCostTotalDto,
  DashboardCumulativeRequestMetricsSnapshotDto,
  DashboardLiveRequestMetricsSnapshotDto,
  DashboardPeriodMetricsDto,
  DashboardRequestMetricsInputDto,
} from "@/lib/bridge/generated";

export type DashboardRequestMetricsInput = DashboardRequestMetricsInputDto;
export type DashboardPeriodMetrics = DashboardPeriodMetricsDto;
export type DashboardCostTotal = DashboardCostTotalDto;
export type DashboardCostMetrics = DashboardCostMetricsDto;
export type DashboardLiveRequestMetricsSnapshot = DashboardLiveRequestMetricsSnapshotDto;
export type DashboardCumulativeRequestMetricsSnapshot =
  DashboardCumulativeRequestMetricsSnapshotDto;
