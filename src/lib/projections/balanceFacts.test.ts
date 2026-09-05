import { describe, expect, it } from "vitest";
import {
  balanceSnapshotState,
  buildCurrentStationBalanceFacts,
  isBalanceSnapshotEligibleForRouting,
} from "./balanceFacts";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";

function station(overrides: Partial<Station> = {}): Station {
  return {
    id: "station-1",
    name: "Relay",
    stationType: "sub2api",
    websiteUrl: "https://console.example.test",
    apiBaseUrl: "https://api.example.test/v1",
    endpointRevision: 1,
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    keyCount: 0,
    enabled: true,
    priority: 0,
    creditPerCny: 1,
    balanceRaw: 123,
    balanceCny: 123,
    lowBalanceThresholdCny: 10,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: "2026-09-03T00:00:00Z",
    lastPricingFetchedAt: "2026-09-03T00:00:00Z",
    note: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-09-03T00:00:00Z",
    ...overrides,
  };
}

describe("buildCurrentStationBalanceFacts", () => {
  it("fails closed when no typed balance snapshot exists", () => {
    const [fact] = [...buildCurrentStationBalanceFacts({ stations: [station()], balances: [] }).values()];

    expect(fact.source).toBe("missing");
    expect(fact.value).toBeNull();
    expect(fact.updatedAt).toBeNull();
    expect(fact.collectedAt).toBeNull();
  });

  it.each([
    ["stale", { validUntilMs: 99 }],
    ["untrusted", { evidenceConfidence: "probable" }],
    ["untrusted", { spendabilityAuthority: "advisory" }],
    ["depleted", { value: 0 }],
    ["missing", { value: null, status: "normal" }],
  ] as const)("classifies %s snapshots without routing eligibility", (expected, overrides) => {
    const snapshot = balanceSnapshot(overrides);
    expect(balanceSnapshotState(snapshot, 100)).toBe(expected);
    expect(isBalanceSnapshotEligibleForRouting(snapshot, 100)).toBe(false);
  });

  it("does not accept a key quota or legacy aggregate as account balance", () => {
    const key = balanceSnapshot({
      stationKeyId: "key-1",
      scope: "station_key",
      balanceKind: "station_key_quota",
    });
    const legacy = balanceSnapshot({
      balanceKind: "legacy_derived_aggregate",
      value: 5.6,
    });
    const facts = buildCurrentStationBalanceFacts({ stations: [station()], balances: [key, legacy] });

    expect(facts.get("station-1")?.state).toBe("missing");
    expect(facts.get("station-1")?.value).toBeNull();
  });
});

function balanceSnapshot(overrides: Partial<BalanceSnapshot> = {}): BalanceSnapshot {
  return {
    id: "account-1",
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
    observedAtMs: 100,
    validUntilMs: null,
    evidenceProfileVersion: "test-v1",
    spendabilityReasonCode: "account_balance",
    createdAt: "2026-09-05T00:00:00Z",
    updatedAt: "2026-09-05T00:00:00Z",
    ...overrides,
  };
}
