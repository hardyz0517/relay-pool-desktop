import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

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
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ pricing: pricing as BackendClient["pricing"] }));
    pricing.loadPricingComparisonWorkspace.mockClear();
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
