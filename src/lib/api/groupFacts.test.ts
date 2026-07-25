import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  listGroupRateRecords,
  listStationGroupBindings,
  listStationGroupOptions,
  upsertStationGroupBinding,
} from "./groupFacts";

describe("group facts backend cutover", () => {
  const groupFacts = {
    listStationGroupBindings: vi.fn(async () => []),
    listStationGroupOptions: vi.fn(async () => []),
    listGroupRateRecords: vi.fn(async () => []),
    upsertStationGroupBinding: vi.fn(async (input) => ({
      id: "binding-1",
      lastCheckedAt: null,
      lastRateChangedAt: null,
      createdAt: "now",
      updatedAt: "now",
      ...input,
    })),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ groupFacts: groupFacts as BackendClient["groupFacts"] }));
    for (const fn of Object.values(groupFacts)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes group reads and writes through the active backend client", async () => {
    const input = {
      stationId: "station-1",
      stationKeyId: null,
      bindingKind: "station_group" as const,
      parentGroupBindingId: null,
      groupKeyHash: "group-key",
      groupIdHash: "group-id",
      groupName: "Fixture",
      bindingStatus: "available" as const,
      defaultRateMultiplier: 1,
      userRateMultiplier: null,
      effectiveRateMultiplier: 1,
      inferredGroupCategory: null,
      groupCategoryOverride: null,
      rateSource: "fixture",
      confidence: 1,
      lastSeenAt: null,
      rawJsonRedacted: null,
    };

    await listStationGroupBindings("station-1");
    await listStationGroupOptions("station-1");
    await listGroupRateRecords("station-1");
    await upsertStationGroupBinding(input);

    expect(groupFacts.listStationGroupBindings).toHaveBeenCalledWith("station-1");
    expect(groupFacts.listStationGroupOptions).toHaveBeenCalledWith("station-1");
    expect(groupFacts.listGroupRateRecords).toHaveBeenCalledWith("station-1");
    expect(groupFacts.upsertStationGroupBinding).toHaveBeenCalledWith(input);
  });
});

function testBackendClient(overrides: Partial<BackendClient>): BackendClient {
  return {
    mode: "desktop",
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    changeEvents: {} as BackendClient["changeEvents"],
    collectorRuns: {} as BackendClient["collectorRuns"],
    proxy: {} as BackendClient["proxy"],
    localRouting: {} as BackendClient["localRouting"],
    dataRecovery: {} as BackendClient["dataRecovery"],
    economics: {} as BackendClient["economics"],
    groupFacts: {} as BackendClient["groupFacts"],
    pricing: {} as BackendClient["pricing"],
    routing: {} as BackendClient["routing"],
    channels: {} as BackendClient["channels"],
    handshake: vi.fn(async () => ({}) as never),
    ...overrides,
  };
}
