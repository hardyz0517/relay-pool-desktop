import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";
import { pricingGroupMonitorStatusQueryOptions } from "@/lib/query/resourceQueries";

import { loadPricingComparisonWorkspace } from "./pricingQueries";

describe("pricing workspace backend cutover", () => {
  const pricing = {
    loadPricingComparisonWorkspace: vi.fn(async () => ({
      stations: [],
      stationKeys: [],
      groupBindings: [],
      groupRates: [],
      pricingRules: [],
      developerModeEnabled: false,
    })),
    loadPricingGroupMonitorStatus: vi.fn(async () => ({
      schemaVersion: 1 as const,
      generatedAtMs: 0,
      groupRefsHash: "",
      requestedGroupCount: 0,
      returnedGroupCount: 0,
      omittedGroupCount: 0,
      items: [],
    })),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ pricing: pricing as BackendClient["pricing"] }));
    pricing.loadPricingComparisonWorkspace.mockClear();
    pricing.loadPricingGroupMonitorStatus.mockClear();
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("loads the pricing workspace through the active backend client", async () => {
    await expect(loadPricingComparisonWorkspace()).resolves.toMatchObject({
      stations: [],
      stationKeys: [],
      groupBindings: [],
      groupRates: [],
      pricingRules: [],
      developerModeEnabled: false,
    });

    expect(pricing.loadPricingComparisonWorkspace).toHaveBeenCalledTimes(1);
  });

  it("keeps optional monitoring projection failures out of global error notifications", () => {
    const options = pricingGroupMonitorStatusQueryOptions(
      { schemaVersion: 1, groupRefsHash: "fixture", groups: [] },
      true,
    );

    expect(options.meta).toEqual({ suppressGlobalErrorNotification: true });
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
    collectors: {} as BackendClient["collectors"],
    proxy: {} as BackendClient["proxy"],
    dashboard: {} as BackendClient["dashboard"],
    runtime: {} as BackendClient["runtime"],
    dataRecovery: {} as BackendClient["dataRecovery"],
    dataMigration: {} as BackendClient["dataMigration"],
    economics: {} as BackendClient["economics"],
    groupFacts: {} as BackendClient["groupFacts"],
    pricing: {} as BackendClient["pricing"],
    routing: {} as BackendClient["routing"],
    channels: {} as BackendClient["channels"],
    updater: {} as BackendClient["updater"],
    handshake: vi.fn(async () => ({}) as never),
    ...overrides,
  };
}
