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

export type TodayCostCompleteness = {
  priced: number;
  missingUsage: number;
  missingPrice: number;
  missingAggregate: number;
};

export function todayCostCompleteness(
  costs: DashboardCostMetrics,
  missingUsageRequestCount: number,
): TodayCostCompleteness {
  return {
    priced: costs.completeSingleCurrencyCount + costs.completeMixedCurrencyCount,
    missingUsage: missingUsageRequestCount,
    missingPrice: Math.max(0, costs.incompleteCount - missingUsageRequestCount),
    missingAggregate: costs.legacyOrMissingAggregateCount,
  };
}

export function formatTodayCostCompleteness(summary: TodayCostCompleteness) {
  return [`已计价 ${summary.priced}`, ...todayCostGapParts(summary)].join(" · ");
}

export function todayCostGapParts(summary: TodayCostCompleteness) {
  const parts: string[] = [];
  if (summary.missingUsage > 0) parts.push(`缺用量 ${summary.missingUsage}`);
  if (summary.missingPrice > 0) parts.push(`缺基准价 ${summary.missingPrice}`);
  if (summary.missingAggregate > 0) parts.push(`缺汇总 ${summary.missingAggregate}`);
  return parts;
}

export function formatCumulativeCostHover(
  summary: TodayCostCompleteness | null,
  lifetimeGaps: number,
) {
  const parts: string[] = [];
  if (summary) parts.push(formatTodayCostCompleteness(summary));
  if (lifetimeGaps > 0) parts.push(`历史成本缺口 ${lifetimeGaps} 条`);
  return parts.join(" · ");
}

export function lifetimeCostGapCount(metrics: DashboardCostMetrics) {
  return metrics.incompleteCount + metrics.legacyOrMissingAggregateCount;
}
