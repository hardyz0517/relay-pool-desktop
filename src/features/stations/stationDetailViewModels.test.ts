import { describe, expect, it } from "vitest";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { Station } from "@/lib/types/stations";
import { buildMetricCards, buildStationDetailViewModel } from "./stationDetailViewModels";

function station(stationType: Station["stationType"]): Station {
  return {
    id: "station-1",
    name: "Relay",
    stationType,
    websiteUrl: "https://console.example",
    apiBaseUrl: "https://api.example/v1",
    endpointRevision: 1,
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 1,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: null,
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-08-01T00:00:00Z",
    updatedAt: "2026-08-01T00:00:00Z",
  };
}

function balance(overrides: Partial<BalanceSnapshot> = {}): BalanceSnapshot {
  return {
    id: "balance-1",
    stationId: "station-1",
    stationKeyId: null,
    scope: "station",
    value: 8,
    currency: "USD",
    creditUnit: null,
    usedValue: 2,
    totalValue: 10,
    todayRequestCount: 34,
    totalRequestCount: 1200,
    todayConsumption: 1.25,
    totalConsumption: 12.5,
    todayBaseConsumption: null,
    totalBaseConsumption: null,
    todayTokenCount: 43210,
    totalTokenCount: 987654,
    todayInputTokenCount: 30000,
    todayOutputTokenCount: 13210,
    totalInputTokenCount: 700000,
    totalOutputTokenCount: 287654,
    accountConcurrencyLimit: 16,
    lowBalanceThreshold: null,
    status: "normal",
    source: "newapi_user_self",
    confidence: 0.95,
    collectedAt: "2026-08-01T01:00:00Z",
    createdAt: "2026-08-01T01:00:00Z",
    updatedAt: "2026-08-01T01:00:00Z",
    ...overrides,
  };
}

describe("buildMetricCards", () => {
  it("uses the requested two-row metric order", () => {
    const cards = buildMetricCards(station("sub2api"), [balance()]);

    expect(cards.map((card) => card.label)).toEqual([
      "当前余额",
      "今日消费",
      "并发限制",
      "今日请求",
      "今日 Token",
      "累计 Token",
    ]);
  });

  it("treats NewAPI concurrency as unlimited instead of missing collection data", () => {
    const cards = buildMetricCards(station("newapi"), [balance({ accountConcurrencyLimit: null })]);
    const concurrency = cards.find((card) => card.label === "并发限制");
    const totalTokens = cards.find((card) => card.label === "累计 Token");

    expect(concurrency).toMatchObject({
      value: "无限制",
      tone: "neutral",
    });
    expect(concurrency?.helper).not.toContain("未采集");
    expect(totalTokens).toMatchObject({
      value: "无法计算",
      helper: "NewAPI 不提供账号累计Token",
      tone: "neutral",
    });
  });
});

describe("buildStationDetailViewModel", () => {
  it("uses the latest top-level collector run instead of a stale station status", () => {
    const currentStation = { ...station("sub2api"), status: "unchecked" as const };
    const parentRun = collectorRun({ id: "parent", status: "partial" });
    const childRun = collectorRun({
      id: "child",
      parentRunId: parentRun.id,
      status: "failed",
      startedAt: "2026-08-01T02:00:00Z",
      finishedAt: "2026-08-01T02:01:00Z",
    });

    const viewModel = buildStationDetailViewModel({
      station: currentStation,
      balances: [],
      groupBindings: [],
      groupRates: [],
      collectorRuns: [childRun, parentRun],
      latestSnapshot: null,
      credentials: null,
      stationKeys: [],
      incidents: [],
    });

    expect(viewModel.statusLabel).toBe("采集需关注");
    expect(viewModel.statusTone).toBe("warning");
    expect(viewModel.collectorItems[0]?.value).toBe("全量 · 部分成功");
  });
});

function collectorRun(overrides: Partial<CollectorRun> = {}): CollectorRun {
  return {
    id: "run-1",
    stationId: "station-1",
    parentRunId: null,
    adapter: "sub2api",
    taskType: "full",
    status: "success",
    startedAt: "2026-08-01T01:00:00Z",
    finishedAt: "2026-08-01T01:01:00Z",
    durationMs: 60_000,
    endpointCount: 3,
    successCount: 3,
    failureCount: 0,
    manualActionRequired: false,
    errorCode: null,
    errorMessage: null,
    snapshotId: "snapshot-1",
    createdAt: "2026-08-01T01:00:00Z",
    ...overrides,
  };
}
