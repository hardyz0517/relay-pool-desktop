import type {
  DashboardCostMetricsDto,
  DashboardCostTotalDto,
  DashboardCumulativeRequestMetricsSnapshotDto,
  DashboardLiveRequestMetricsSnapshotDto,
  DashboardPeriodMetricsDto,
  DashboardRecentMetricsDto,
  DashboardRequestMetricsInputDto,
} from "@/lib/bridge/generated";

export const DASHBOARD_REQUEST_METRICS_SCHEMA_VERSION = 1;

export type DashboardRequestMetricsInput = DashboardRequestMetricsInputDto;
export type DashboardPeriodMetrics = DashboardPeriodMetricsDto;
export type DashboardRecentMetrics = DashboardRecentMetricsDto;
export type DashboardCostTotal = DashboardCostTotalDto;
export type DashboardCostMetrics = DashboardCostMetricsDto;
export type DashboardLiveRequestMetricsSnapshot = DashboardLiveRequestMetricsSnapshotDto;
export type DashboardCumulativeRequestMetricsSnapshot =
  DashboardCumulativeRequestMetricsSnapshotDto;
