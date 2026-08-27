import type {
  DashboardCostMetrics,
  DashboardCostTotal,
  DashboardRequestMetricsInput,
} from "@/lib/types/dashboardMetrics";

export function getLocalDayMetricsInput(now = new Date()): DashboardRequestMetricsInput {
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  const end = new Date(start);
  end.setDate(end.getDate() + 1);
  return {
    localDayStartMs: start.getTime(),
    localDayEndMs: end.getTime(),
  };
}

export function msUntilNextLocalDay(now = new Date()) {
  const input = getLocalDayMetricsInput(now);
  return Math.max(1_000, input.localDayEndMs - now.getTime() + 25);
}

export function amountMicroToMajorUnits(total: DashboardCostTotal) {
  return total.amountMicro / 1_000_000;
}

export function hasCostQualityIssue(metrics: DashboardCostMetrics) {
  return !metrics.costTotalsComplete ||
    metrics.incompleteCount > 0 ||
    metrics.legacyOrMissingAggregateCount > 0;
}
