import { describe, expect, it } from "vitest";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { Station } from "@/lib/types/stations";
import {
  buildStationAssetRows,
  hasPositiveBalance,
  stationIssueTags,
  STATION_ISSUE_FILTER_OPTIONS,
} from "./stationAssetViewModels";

describe("hasPositiveBalance", () => {
  it.each([
    [null, false],
    [undefined, false],
    [0, false],
    [-0.01, false],
    [Number.NaN, false],
    [Number.POSITIVE_INFINITY, false],
    [0.01, true],
  ])("classifies %s as %s", (value, expected) => {
    expect(hasPositiveBalance(value)).toBe(expected);
  });
});

describe("station issue filters", () => {
  it("does not treat a missing station API key as an issue", () => {
    expect(STATION_ISSUE_FILTER_OPTIONS.map((option) => option.label)).not.toContain("缺 API 密钥");
  });
});

describe("station collection issue tags", () => {
  it("does not let an isolated failed snapshot override a healthy station", () => {
    const tags = issueTagsFor(station(), snapshot({ status: "failed" }));

    expect(tags.map((tag) => tag.kind)).not.toContain("collection_failed");
  });

  it("does not let isolated authorization metadata override a healthy station", () => {
    const tags = issueTagsFor(
      station(),
      snapshot({
        status: "manual_required",
        summaryJson: { loginRequired: true },
      }),
    );

    expect(tags.map((tag) => tag.kind)).not.toContain("login_required");
  });

  it("keeps a station unchecked when only an isolated snapshot exists", () => {
    const tags = issueTagsFor(
      station({ status: "unchecked" }),
      snapshot({ status: "success" }),
    );

    expect(tags.map((tag) => tag.kind)).toContain("not_collected");
  });

  it("reports the revision-fenced station error even if the newest snapshot succeeded", () => {
    const tags = issueTagsFor(
      station({ status: "error" }),
      snapshot({ status: "success" }),
    );

    expect(tags).toContainEqual(expect.objectContaining({ kind: "collection_failed" }));
  });

  it("retains a generic warning when the latest snapshot cannot explain it", () => {
    const tags = issueTagsFor(
      station({ status: "warning" }),
      snapshot({ status: "success" }),
    );

    expect(tags).toContainEqual(expect.objectContaining({ kind: "collection_warning" }));
  });

  it("refines a station warning when the latest core snapshot requires authorization", () => {
    const tags = issueTagsFor(
      station({ status: "warning" }),
      snapshot({
        status: "manual_required",
        summaryJson: { loginRequired: true },
      }),
    );

    expect(tags).toContainEqual(expect.objectContaining({ kind: "login_required" }));
  });
});

function issueTagsFor(currentStation: Station, latestSnapshot: CollectorSnapshot | null) {
  const [row] = buildStationAssetRows({
    stations: [currentStation],
    keysByStation: new Map(),
    balances: [],
    snapshotsByStation: new Map([[currentStation.id, latestSnapshot]]),
    groupBindingsByStation: new Map(),
    incidents: [],
    balanceFactsReady: false,
  });
  return stationIssueTags(row);
}

function station(overrides: Partial<Station> = {}): Station {
  return {
    id: "station-1",
    name: "Relay",
    stationType: "sub2api",
    websiteUrl: "https://console.example",
    apiBaseUrl: "https://api.example/v1",
    endpointRevision: 1,
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    keyCount: 0,
    enabled: true,
    priority: 0,
    creditPerCny: 1,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: "2026-08-27T10:34:00Z",
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-08-27T10:00:00Z",
    updatedAt: "2026-08-27T10:34:00Z",
    ...overrides,
  };
}

function snapshot(overrides: Partial<CollectorSnapshot> = {}): CollectorSnapshot {
  return {
    id: "snapshot-1",
    stationId: "station-1",
    endpointRevision: 1,
    source: "sub2api",
    status: "success",
    fetchedAt: "2026-08-27T10:34:00Z",
    summaryJson: {},
    normalizedJson: {},
    rawJsonRedacted: null,
    errorMessage: null,
    createdAt: "2026-08-27T10:34:00Z",
    ...overrides,
  };
}
