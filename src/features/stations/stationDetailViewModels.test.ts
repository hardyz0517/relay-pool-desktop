import { describe, expect, it } from "vitest";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { StationGroupBinding } from "@/lib/types/groupFacts";
import type { Station } from "@/lib/types/stations";
import { buildGroupRows, buildMetricCards, buildStationDetailViewModel } from "./stationDetailViewModels";

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
    collectionSummary: {
      status: "healthy",
      reasonCodes: [],
      revision: 1,
    },
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
    balanceKind: "account_balance",
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
    evidenceConfidence: "confirmed",
    spendabilityAuthority: "authoritative",
    observedAtMs: Date.parse("2026-08-01T01:00:00Z"),
    validUntilMs: null,
    evidenceProfileVersion: "test-v1",
    spendabilityReasonCode: "test",
    createdAt: "2026-08-01T01:00:00Z",
    updatedAt: "2026-08-01T01:00:00Z",
    ...overrides,
  };
}

describe("buildMetricCards", () => {
  it("uses the requested two-row metric order", () => {
    const cards = buildMetricCards(station("sub2api"), [balance()]);

    expect(cards.map((card) => card.label)).toEqual([
      "账号余额",
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

describe("buildGroupRows", () => {
  it("labels a collected default multiplier as station collection", () => {
    const [row] = buildGroupRows([groupBinding({ description: "Default models" })], []);

    expect(row).toMatchObject({
      effectiveRate: "0.85x",
      rateSource: "站点采集",
      description: "Default models",
    });
  });

  it("labels a user multiplier as a manual override", () => {
    const [row] = buildGroupRows(
      [
        groupBinding({
          defaultRateMultiplier: 0.85,
          userRateMultiplier: 0.4,
          effectiveRateMultiplier: 0.4,
        }),
      ],
      [],
    );

    expect(row).toMatchObject({
      effectiveRate: "0.4x",
      rateSource: "手动覆盖",
    });
  });

  it("keeps the source explicit when no current multiplier was collected", () => {
    const [row] = buildGroupRows(
      [
        groupBinding({
          defaultRateMultiplier: null,
          userRateMultiplier: null,
          effectiveRateMultiplier: null,
        }),
      ],
      [],
    );

    expect(row?.rateSource).toBe("未采集");
  });
});

describe("buildStationDetailViewModel", () => {
  it("keeps the revision-fenced station status independent from the latest task", () => {
    const currentStation = station("sub2api");
    const publishedStatusRun = collectorRun({
      id: "published-status",
      taskType: "published_status",
      status: "failed",
      startedAt: "2026-08-01T02:00:00Z",
      finishedAt: "2026-08-01T02:01:00Z",
    });

    const viewModel = buildStationDetailViewModel({
      station: currentStation,
      balances: [],
      groupBindings: [],
      groupRates: [],
      collectorRuns: [publishedStatusRun],
      latestSnapshot: null,
      credentials: null,
      stationKeys: [],
      incidents: [],
    });

    expect(viewModel.statusLabel).toBe("采集正常");
    expect(viewModel.statusTone).toBe("good");
    expect(viewModel.collectorItems[0]?.value).toBe("官方状态 · 失败");
  });

  it("renders authorization expiry diagnostics in Chinese", () => {
    const viewModel = buildStationDetailViewModel({
      station: station("sub2api"),
      balances: [],
      groupBindings: [],
      groupRates: [],
      collectorRuns: [],
      latestSnapshot: null,
      credentials: null,
      stationKeys: [],
      incidents: [
        {
          id: "incident-authorization",
          conditionKey: "collector:station-1:authorization_expired",
          eventType: "authorization_expired",
          lifecycleState: "open",
          severity: "warning",
          groupName: null,
          stationId: "station-1",
          episodeNumber: 1,
          firstSeenAtMs: 1,
          occurrenceCount: 1,
          lastSeenAtMs: 1,
          collectorFailedTaskTypes: [],
          resolvedAtMs: null,
          updatedAtMs: 1,
          seenAtMs: null,
          snoozedUntilMs: null,
        },
      ],
    });

    expect(viewModel.changeItems[0]?.label).toBe("授权过期");
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

function groupBinding(overrides: Partial<StationGroupBinding> = {}): StationGroupBinding {
  return {
    id: "binding-1",
    stationId: "station-1",
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: "group-hash",
    groupIdHash: "group-id-hash",
    groupName: "default",
    description: null,
    bindingStatus: "available",
    defaultRateMultiplier: 0.85,
    userRateMultiplier: null,
    effectiveRateMultiplier: 0.85,
    inferredGroupCategory: null,
    groupCategoryOverride: null,
    rateSource: "remote_scan",
    confidence: 1,
    lastSeenAt: "2026-08-01T01:00:00Z",
    lastCheckedAt: "2026-08-01T01:00:00Z",
    lastRateChangedAt: null,
    rawJsonRedacted: null,
    createdAt: "2026-08-01T01:00:00Z",
    updatedAt: "2026-08-01T01:00:00Z",
    ...overrides,
  };
}
