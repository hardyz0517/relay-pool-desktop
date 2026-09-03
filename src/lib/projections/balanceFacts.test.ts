import { describe, expect, it } from "vitest";
import { buildCurrentStationBalanceFacts } from "./balanceFacts";
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
});
