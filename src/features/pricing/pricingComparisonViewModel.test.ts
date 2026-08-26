import { describe, expect, it } from "vitest";
import type { Station } from "@/lib/types/stations";
import type { StationKey } from "@/lib/types/stationKeys";
import type { StationGroupBinding } from "@/lib/types/groupFacts";
import type { PricingGroupMonitorSummary, PricingGroupMonitorStatusWorkspace } from "@/lib/types/pricingMonitoring";
import {
  buildPricingMonitorRefs,
  buildPricingComparisonViewModel,
  type PricingComparisonInput,
} from "./pricingComparisonViewModel";

const station: Station = {
  id: "station-1",
  name: "Station One",
  stationType: "sub2api",
  websiteUrl: "https://station.invalid",
  apiBaseUrl: "https://station.invalid/v1",
  endpointRevision: 1,
  collectorProxyMode: "inherit",
  collectorProxyUrl: null,
  apiKeyMasked: "",
  apiKeyPresent: false,
  keyCount: 2,
  enabled: true,
  priority: 1,
  creditPerCny: 1,
  balanceRaw: null,
  balanceCny: null,
  lowBalanceThresholdCny: null,
  collectionIntervalMinutes: 60,
  status: "healthy",
  latencyMs: null,
  lastCheckedAt: null,
  lastPricingFetchedAt: null,
  note: null,
  createdAt: "1",
  updatedAt: "1",
};

function key(id: string, apiKeyPresent = true): StationKey {
  return {
    id,
    stationId: station.id,
    name: id,
    apiKeyMasked: apiKeyPresent ? "****" : "",
    apiKeyPresent,
    enabled: true,
    priority: 1,
    maxConcurrency: 1,
    loadFactor: null,
    schedulable: true,
    groupBindingId: "binding-1",
    groupIdHash: "group-1",
    groupName: "GPT",
    tierLabel: null,
    rateMultiplier: null,
    manualRateMultiplier: null,
    manualRateUpdatedAt: null,
    rateSource: null,
    rateCollectedAt: null,
    balanceScope: null,
    status: "healthy",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: null,
    createdAt: "1",
    updatedAt: "1",
  };
}

function binding(id: string, multiplier: number): StationGroupBinding {
  return {
    id,
    stationId: station.id,
    stationKeyId: id === "binding-1" ? "key-1" : null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: `group-key-${id}`,
    groupIdHash: `group-id-${id}`,
    groupName: id === "binding-1" ? "GPT" : "GPT Backup",
    bindingStatus: "available",
    defaultRateMultiplier: multiplier,
    userRateMultiplier: null,
    effectiveRateMultiplier: multiplier,
    inferredGroupCategory: "gpt",
    groupCategoryOverride: null,
    rateSource: "fixture",
    confidence: 1,
    lastSeenAt: null,
    lastCheckedAt: "2026-08-03T00:00:00.000Z",
    lastRateChangedAt: null,
    rawJsonRedacted: null,
    createdAt: "1",
    updatedAt: "1",
  };
}

function summary(
  bindingId: string,
  displayState: PricingGroupMonitorSummary["displayState"],
): PricingGroupMonitorSummary {
  return {
    stationId: station.id,
    groupBindingId: bindingId,
    groupIdHash: `group-id-${bindingId}`,
    groupKeyHash: `group-key-${bindingId}`,
    matchKind: "exact_binding",
    resolutionState: "resolved",
    hasBoundKey: bindingId === "binding-1",
    boundKeyCount: bindingId === "binding-1" ? 1 : 0,
    enabledKeyCount: bindingId === "binding-1" ? 1 : 0,
    credentialedKeyCount: bindingId === "binding-1" ? 1 : 0,
    enabledMonitorDefinitionCount: bindingId === "binding-1" ? 1 : 0,
    monitoredKeyCount: bindingId === "binding-1" ? 1 : 0,
    testedKeyCount: bindingId === "binding-1" ? 1 : 0,
    representativeKeyId: bindingId === "binding-1" ? "key-1" : null,
    representativeMonitorId: bindingId === "binding-1" ? "monitor-1" : null,
    latestTargetResultId: bindingId === "binding-1" ? "result-1" : null,
    latestOutcome: displayState === "available" ? "available" : "missing",
    latestFailureKind: null,
    latestTerminalReason: null,
    running: displayState === "running",
    checkedAtMs: null,
    latencyMs: null,
    generatedAtMs: 1,
    displayState,
  };
}

function workspace(items: PricingGroupMonitorSummary[]): PricingGroupMonitorStatusWorkspace {
  return {
    schemaVersion: 1,
    generatedAtMs: 1,
    groupRefsHash: "fixture",
    requestedGroupCount: items.length,
    returnedGroupCount: items.length,
    omittedGroupCount: 0,
    items,
  };
}

function input(overrides: Partial<PricingComparisonInput> = {}): PricingComparisonInput {
  return {
    stations: [station],
    stationKeys: [key("key-1")],
    groupBindings: [binding("binding-1", 0.8), binding("binding-2", 1.2)],
    groupRates: [],
    developerModeEnabled: false,
    ...overrides,
  };
}

describe("buildPricingComparisonViewModel", () => {
  it("keeps frontend canonical keys out of the strict IPC payload", () => {
    const refs = buildPricingMonitorRefs(input());

    expect(refs).toEqual([
      {
        stationId: "station-1",
        groupBindingId: "binding-1",
        groupIdHash: "group-id-binding-1",
        groupKeyHash: "group-key-binding-1",
      },
      {
        stationId: "station-1",
        groupBindingId: "binding-2",
        groupIdHash: "group-id-binding-2",
        groupKeyHash: "group-key-binding-2",
      },
    ]);
    expect(refs.every((ref) => !("canonicalKey" in ref))).toBe(true);
  });

  it("merges summaries without changing price order", () => {
    const model = buildPricingComparisonViewModel(
      input({
        monitorWorkspace: workspace([
          summary("binding-1", "unavailable"),
          summary("binding-2", "available"),
        ]),
      }),
    );
    const rows = model.sections.flatMap((section) => section.rows);

    expect(rows.map((row) => row.groupBindingId)).toEqual(["binding-1", "binding-2"]);
    expect(rows.map((row) => row.monitorDisplayState)).toEqual(["unavailable", "available"]);
  });

  it("applies key, monitor, and outcome filters with AND semantics", () => {
    const model = buildPricingComparisonViewModel(
      input({
        monitorWorkspace: workspace([
          summary("binding-1", "available"),
          summary("binding-2", "unmonitored"),
        ]),
        filters: {
          keyPresence: "with_credentialed_key",
          monitorPresence: "monitored",
          monitorOutcome: "success",
        },
      }),
    );

    expect(model.sections.flatMap((section) => section.rows).map((row) => row.groupBindingId)).toEqual([
      "binding-1",
    ]);
  });

  it("keeps unavailable monitor data out of success and failure filters", () => {
    const ready = buildPricingComparisonViewModel(input({ monitorDataState: "error" }));
    const success = buildPricingComparisonViewModel(
      input({ monitorDataState: "error", filters: { monitorOutcome: "success" } }),
    );
    const failure = buildPricingComparisonViewModel(
      input({ monitorDataState: "error", filters: { monitorOutcome: "failure" } }),
    );

    expect(ready.sections.flatMap((section) => section.rows)).toHaveLength(2);
    expect(success.sections).toEqual([]);
    expect(failure.sections).toEqual([]);
  });

  it("supports explicit degraded, skipped, unavailable-data, and unresolved filters", () => {
    const monitorWorkspace = workspace([
      summary("binding-1", "degraded"),
      summary("binding-2", "skipped"),
    ]);
    const degraded = buildPricingComparisonViewModel(
      input({ monitorWorkspace, filters: { monitorOutcome: "degraded" } }),
    );
    const skipped = buildPricingComparisonViewModel(
      input({ monitorWorkspace, filters: { monitorOutcome: "skipped" } }),
    );
    const unavailableData = buildPricingComparisonViewModel(
      input({ monitorDataState: "error", filters: { monitorOutcome: "unavailable_data" } }),
    );
    const unresolved = buildPricingComparisonViewModel(
      input({ monitorDataState: "ready", filters: { monitorOutcome: "unresolved" } }),
    );

    expect(degraded.sections.flatMap((section) => section.rows).map((row) => row.groupBindingId)).toEqual([
      "binding-1",
    ]);
    expect(skipped.sections.flatMap((section) => section.rows).map((row) => row.groupBindingId)).toEqual([
      "binding-2",
    ]);
    expect(unavailableData.sections.flatMap((section) => section.rows)).toHaveLength(2);
    expect(unresolved.sections.flatMap((section) => section.rows)).toHaveLength(2);
  });

  it("does not treat monitor query failure as unmonitored", () => {
    const model = buildPricingComparisonViewModel(
      input({
        monitorDataState: "error",
        filters: { monitorPresence: "unmonitored" },
      }),
    );
    expect(model.sections).toEqual([]);
  });
});
