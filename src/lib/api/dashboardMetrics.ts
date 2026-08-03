import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  DashboardCumulativeRequestMetricsSnapshot,
  DashboardLiveRequestMetricsSnapshot,
  DashboardRequestMetricsInput,
} from "@/lib/types/dashboardMetrics";

export function loadDashboardLiveRequestMetrics(
  input: DashboardRequestMetricsInput,
): Promise<DashboardLiveRequestMetricsSnapshot> {
  return getActiveBackendClient().dashboard.loadLiveRequestMetrics(input);
}

export function loadDashboardCumulativeRequestMetrics(): Promise<DashboardCumulativeRequestMetricsSnapshot> {
  return getActiveBackendClient().dashboard.loadCumulativeRequestMetrics();
}
