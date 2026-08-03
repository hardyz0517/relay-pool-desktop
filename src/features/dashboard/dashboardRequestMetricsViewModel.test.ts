import { describe, expect, it } from "vitest";
import {
  amountMicroToMajorUnits,
  getLocalDayMetricsInput,
  hasCostQualityIssue,
  msUntilNextLocalDay,
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
});
