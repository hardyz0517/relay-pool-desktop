import type {
  DashboardCostMetrics,
  DashboardCostTotal,
  DashboardPeriodMetrics,
  DashboardRecentMetrics,
  DashboardRequestMetricsInput,
} from "@/lib/types/dashboardMetrics";

export const emptyDashboardPeriodMetrics: DashboardPeriodMetrics = {
  requestCount: 0,
  terminalCount: 0,
  successCount: 0,
  failedCount: 0,
  interruptedCount: 0,
  inProgressCount: 0,
  promptTokens: 0,
  completionTokens: 0,
  totalTokens: 0,
  knownUsageRequestCount: 0,
  missingUsageRequestCount: 0,
  streamUsageMissingRequestCount: 0,
  notApplicableUsageRequestCount: 0,
  unknownUsageRequestCount: 0,
  totalDurationMs: 0,
  durationSampleCount: 0,
  firstTokenTotalMs: 0,
  firstTokenSampleCount: 0,
  avgTotalDurationMs: null,
  avgFirstTokenMs: null,
};

export const emptyDashboardRecentMetrics: DashboardRecentMetrics = {
  period: emptyDashboardPeriodMetrics,
  windowMinutes: 5,
  rpm: 0,
  tpm: 0,
};

export const emptyDashboardCostMetrics: DashboardCostMetrics = {
  totals: [],
  costTotalsComplete: true,
  completeSingleCurrencyCount: 0,
  completeMixedCurrencyCount: 0,
  incompleteCount: 0,
  notApplicableCount: 0,
  noAttemptsCount: 0,
  legacyOrMissingAggregateCount: 0,
};

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
