import { describe, expect, it } from "vitest";
import { summarizeDashboardBalances } from "./dashboardBalanceSummary";
import type { BalanceSnapshot } from "@/lib/types/economics";

function balance(overrides: Partial<BalanceSnapshot> = {}): BalanceSnapshot {
  return {
    id: "balance-account",
    stationId: "station-1",
    stationKeyId: null,
    scope: "station",
    balanceKind: "account_balance",
    value: 2.8,
    currency: "CNY",
    creditUnit: null,
    usedValue: null,
    totalValue: null,
    todayRequestCount: null,
    totalRequestCount: null,
    todayConsumption: null,
    totalConsumption: null,
    todayBaseConsumption: null,
    totalBaseConsumption: null,
    todayTokenCount: null,
    totalTokenCount: null,
    todayInputTokenCount: null,
    todayOutputTokenCount: null,
    totalInputTokenCount: null,
    totalOutputTokenCount: null,
    accountConcurrencyLimit: null,
    lowBalanceThreshold: null,
    status: "normal",
    source: "sub2api_account_profile",
    confidence: 1,
    collectedAt: "2026-09-05T00:00:00Z",
    evidenceConfidence: "confirmed",
    spendabilityAuthority: "authoritative",
    observedAtMs: 1788566400000,
    validUntilMs: null,
    evidenceProfileVersion: "test-v1",
    spendabilityReasonCode: "account_balance",
    createdAt: "2026-09-05T00:00:00Z",
    updatedAt: "2026-09-05T00:00:00Z",
    ...overrides,
  };
}

describe("summarizeDashboardBalances", () => {
  it("does not turn per-key quotas or legacy aggregates into station balance", () => {
    const summary = summarizeDashboardBalances([
      balance(),
      balance({
        id: "balance-key-1",
        stationKeyId: "key-1",
        scope: "station_key",
        balanceKind: "station_key_quota",
        source: "sub2api_usage",
        updatedAt: "2026-09-05T00:01:00Z",
      }),
      balance({
        id: "balance-key-2",
        stationKeyId: "key-2",
        scope: "station_key",
        balanceKind: "station_key_quota",
        source: "sub2api_usage",
        updatedAt: "2026-09-05T00:02:00Z",
      }),
      balance({
        id: "balance-aggregate",
        balanceKind: "legacy_derived_aggregate",
        source: "station_key_balance_aggregate",
        value: 5.6,
        updatedAt: "2026-09-05T00:03:00Z",
      }),
      balance({
        id: "balance-subscription",
        balanceKind: "subscription_quota",
        scope: "subscription",
        value: 99,
        updatedAt: "2026-09-05T00:04:00Z",
      }),
    ]);

    expect(summary.totalBalance).toBe(2.8);
    expect(summary.latestStationBalances.map((item) => item.id)).toEqual(["balance-account"]);
  });
});
