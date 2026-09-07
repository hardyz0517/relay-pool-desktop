import { describe, expect, it } from "vitest";
import {
  amountMicroToMajorUnits,
  formatCumulativeCostHover,
  formatTodayCostCompleteness,
  getLocalDayMetricsInput,
  hasCostQualityIssue,
  lifetimeCostGapCount,
  msUntilNextLocalDay,
  todayCostCompleteness,
  todayCostGapParts,
} from "./dashboardRequestMetricsViewModel";

describe("dashboardRequestMetricsViewModel", () => {
  it("builds an inclusive local-day start and exclusive local-day end", () => {
    const input = getLocalDayMetricsInput(new Date(2026, 7, 1, 12, 30, 0, 0));

    expect(input.localDayStartMs).toBe(new Date(2026, 7, 1, 0, 0, 0, 0).getTime());
    expect(input.localDayEndMs).toBe(new Date(2026, 7, 2, 0, 0, 0, 0).getTime());
  });

  it("schedules rollover after the next local midnight", () => {
    const now = new Date(2026, 7, 1, 23, 59, 59, 900);

    expect(msUntilNextLocalDay(now)).toBe(1_000);
  });

  it("converts persisted micro-unit costs without base-cost comparison", () => {
    expect(amountMicroToMajorUnits({
      currency: "USD",
      amountMicro: 1_234_567,
      requestCount: 2,
    })).toBe(1.234567);
  });

  it("flags incomplete or legacy dashboard cost aggregates", () => {
    expect(hasCostQualityIssue({
      totals: [],
      costTotalsComplete: true,
      completeSingleCurrencyCount: 1,
      completeMixedCurrencyCount: 0,
      incompleteCount: 0,
      notApplicableCount: 0,
      noAttemptsCount: 0,
      legacyOrMissingAggregateCount: 0,
    })).toBe(false);

    expect(hasCostQualityIssue({
      totals: [],
      costTotalsComplete: false,
      completeSingleCurrencyCount: 0,
      completeMixedCurrencyCount: 0,
      incompleteCount: 1,
      notApplicableCount: 0,
      noAttemptsCount: 0,
      legacyOrMissingAggregateCount: 0,
    })).toBe(true);
  });

  it("explains today cost gaps without treating in-progress as incomplete", () => {
    const summary = todayCostCompleteness({
      totals: [],
      costTotalsComplete: false,
      completeSingleCurrencyCount: 12,
      completeMixedCurrencyCount: 0,
      incompleteCount: 4,
      notApplicableCount: 0,
      noAttemptsCount: 0,
      legacyOrMissingAggregateCount: 1,
    }, 3);
    expect(summary).toEqual({
      priced: 12,
      missingUsage: 3,
      missingPrice: 1,
      missingAggregate: 1,
    });
    expect(formatTodayCostCompleteness(summary)).toBe("已计价 12 · 缺用量 3 · 缺基准价 1 · 缺汇总 1");
    expect(todayCostGapParts(summary)).toEqual(["缺用量 3", "缺基准价 1", "缺汇总 1"]);
    expect(formatTodayCostCompleteness({
      priced: 4,
      missingUsage: 0,
      missingPrice: 0,
      missingAggregate: 0,
    })).toBe("已计价 4");
    expect(todayCostGapParts({
      priced: 4,
      missingUsage: 0,
      missingPrice: 0,
      missingAggregate: 0,
    })).toEqual([]);
    expect(formatCumulativeCostHover(summary, 155)).toBe("已计价 12 · 缺用量 3 · 缺基准价 1 · 缺汇总 1 · 历史成本缺口 155 条");
    expect(formatCumulativeCostHover({
      priced: 422,
      missingUsage: 0,
      missingPrice: 0,
      missingAggregate: 0,
    }, 155)).toBe("已计价 422 · 历史成本缺口 155 条");
    expect(formatCumulativeCostHover({
      priced: 422,
      missingUsage: 19,
      missingPrice: 0,
      missingAggregate: 0,
    }, 0)).toBe("已计价 422 · 缺用量 19");
    expect(formatCumulativeCostHover(null, 0)).toBe("");
    expect(lifetimeCostGapCount({
      totals: [],
      costTotalsComplete: false,
      completeSingleCurrencyCount: 20,
      completeMixedCurrencyCount: 0,
      incompleteCount: 1,
      notApplicableCount: 0,
      noAttemptsCount: 0,
      legacyOrMissingAggregateCount: 2,
    })).toBe(3);
  });
});
