import type { RequestLog } from "@/lib/types/proxy";

export type DashboardCostTotal = {
  currency: string;
  totalCost: number;
  baseTotalCost: number;
  requestCount: number;
};

export type DashboardRequestCostStatusCounts = {
  priced: number;
  basePriceOnly: number;
  missingRate: number;
  missingModelPrice: number;
  unpriced: number;
  unsupportedBillingMode: number;
  legacyEstimate: number;
  usageOnly: number;
  unknownUsage: number;
};

export type DashboardRequestCostSummary = {
  todayTotalsByCurrency: DashboardCostTotal[];
  allTotalsByCurrency: DashboardCostTotal[];
  statusCounts: DashboardRequestCostStatusCounts;
  unpricedCount: number;
  legacyEstimateCount: number;
  unsupportedBillingModeCount: number;
};

export function summarizeDashboardRequestCosts(
  requestLogs: RequestLog[],
  today: Date = new Date(),
): DashboardRequestCostSummary {
  const startOfToday = new Date(today);
  startOfToday.setHours(0, 0, 0, 0);
  const endOfToday = new Date(startOfToday);
  endOfToday.setDate(endOfToday.getDate() + 1);

  const statusCounts = createStatusCounts();
  const todayTotals = new Map<string, DashboardCostTotal>();
  const allTotals = new Map<string, DashboardCostTotal>();

  for (const log of requestLogs) {
    incrementStatusCount(statusCounts, log.costStatus);
    addCost(allTotals, log);
    if (isWithinDay(log.startedAt, startOfToday, endOfToday)) {
      addCost(todayTotals, log);
    }
  }

  return {
    todayTotalsByCurrency: sortTotals(todayTotals),
    allTotalsByCurrency: sortTotals(allTotals),
    statusCounts,
    unpricedCount:
      statusCounts.missingRate +
      statusCounts.missingModelPrice +
      statusCounts.unpriced +
      statusCounts.unsupportedBillingMode,
    legacyEstimateCount: statusCounts.legacyEstimate,
    unsupportedBillingModeCount: statusCounts.unsupportedBillingMode,
  };
}

function createStatusCounts(): DashboardRequestCostStatusCounts {
  return {
    priced: 0,
    basePriceOnly: 0,
    missingRate: 0,
    missingModelPrice: 0,
    unpriced: 0,
    unsupportedBillingMode: 0,
    legacyEstimate: 0,
    usageOnly: 0,
    unknownUsage: 0,
  };
}

function incrementStatusCount(counts: DashboardRequestCostStatusCounts, status: string | null) {
  switch (status) {
    case "priced":
      counts.priced += 1;
      break;
    case "base_price_only":
      counts.basePriceOnly += 1;
      break;
    case "missing_rate":
      counts.missingRate += 1;
      break;
    case "missing_model_price":
      counts.missingModelPrice += 1;
      break;
    case "unsupported_billing_mode":
      counts.unsupportedBillingMode += 1;
      break;
    case "legacy_estimate":
      counts.legacyEstimate += 1;
      break;
    case "usage_only":
      counts.usageOnly += 1;
      break;
    case "unknown_usage":
      counts.unknownUsage += 1;
      break;
    default:
      counts.unpriced += 1;
      break;
  }
}

function addCost(totals: Map<string, DashboardCostTotal>, log: RequestLog) {
  const cost = log.estimatedTotalCost;
  const baseCost = log.baseTotalCost;
  const hasCost = typeof cost === "number" && Number.isFinite(cost);
  const hasBaseCost = typeof baseCost === "number" && Number.isFinite(baseCost);
  if (!hasCost && !hasBaseCost) {
    return;
  }
  const currency = normalizeCurrency(log.costCurrency);
  const current = totals.get(currency) ?? {
    currency,
    totalCost: 0,
    baseTotalCost: 0,
    requestCount: 0,
  };
  if (hasCost) {
    current.totalCost += cost;
  }
  if (hasBaseCost) {
    current.baseTotalCost += baseCost;
  }
  current.requestCount += 1;
  totals.set(currency, current);
}

function normalizeCurrency(currency: string | null) {
  const normalized = currency?.trim().toUpperCase();
  return normalized || "USD";
}

function sortTotals(totals: Map<string, DashboardCostTotal>) {
  return Array.from(totals.values()).sort((left, right) => left.currency.localeCompare(right.currency));
}

function isWithinDay(value: string, start: Date, end: Date) {
  const time = new Date(value).getTime();
  return Number.isFinite(time) && time >= start.getTime() && time < end.getTime();
}
