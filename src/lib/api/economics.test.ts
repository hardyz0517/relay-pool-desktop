import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";

import {
  deleteModelBasePrice,
  listModelPriceSyncCatalog,
  openModelPriceCatalogDirectory,
  resetModelBasePricesToBuiltins,
  upsertModelBasePrice,
} from "./economics";

describe("pricing mutation backend cutover", () => {
  const economics = {
    resolveStationKeyPricingContext: vi.fn(async () => ({}) as never),
    listModelBasePrices: vi.fn(async () => []),
    listModelPriceSyncCatalog: vi.fn(async () => []),
    upsertModelBasePrice: vi.fn(async (input) => ({ id: "base-1", createdAt: "now", updatedAt: "now", ...input })),
    deleteModelBasePrice: vi.fn(async () => undefined),
    resetModelBasePricesToBuiltins: vi.fn(async () => []),
    getModelPriceSyncState: vi.fn(async () => ({}) as never),
    saveModelPriceSyncConfig: vi.fn(async (input) => input),
    syncModelPrices: vi.fn(async () => ({}) as never),
    reloadModelPriceCatalog: vi.fn(async () => ({}) as never),
    openModelPriceCatalogDirectory: vi.fn(async () => undefined),
    listBalanceSnapshots: vi.fn(async () => []),
    listCurrentStationBalanceSnapshots: vi.fn(async () => []),
    listBalanceSnapshotsForStation: vi.fn(async () => []),
    upsertBalanceSnapshot: vi.fn(async (input) => ({ id: "balance-1", createdAt: "now", updatedAt: "now", ...input })),
  };

  beforeEach(() => {
    setActiveBackendClient(testBackendClient({ economics: economics as BackendClient["economics"] }));
    for (const fn of Object.values(economics)) {
      fn.mockClear();
    }
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes pricing mutations through the active backend client", async () => {
    const basePrice = {
      id: null,
      provider: "openai",
      model: "fixture-model",
      inputPrice: 1,
      outputPrice: 2,
      inputPricePriority: null,
      outputPricePriority: null,
      cacheCreationPrice: 1.25,
      cacheCreationPricePriority: null,
      cacheCreationPriceAbove1Hr: null,
      cacheReadPrice: 0.1,
      cacheReadPricePriority: null,
      longContextInputTokenThreshold: null,
      longContextInputCostMultiplier: null,
      longContextOutputCostMultiplier: null,
      supportsServiceTier: false,
      supportsPromptCaching: true,
      currency: "USD",
      unit: "M",
      sourceUrl: "https://example.test/pricing",
      sourceLabel: "Fixture",
      sourceCheckedAt: null,
      enabled: true,
      builtIn: false,
      note: null,
    };
    await upsertModelBasePrice(basePrice);
    await deleteModelBasePrice("base-1");
    await resetModelBasePricesToBuiltins();
    await listModelPriceSyncCatalog();
    await openModelPriceCatalogDirectory();

    expect(economics.upsertModelBasePrice).toHaveBeenCalledWith(basePrice);
    expect(economics.deleteModelBasePrice).toHaveBeenCalledWith("base-1");
    expect(economics.resetModelBasePricesToBuiltins).toHaveBeenCalledTimes(1);
    expect(economics.listModelPriceSyncCatalog).toHaveBeenCalledTimes(1);
    expect(economics.openModelPriceCatalogDirectory).toHaveBeenCalledTimes(1);
  });
});

function testBackendClient(overrides: Partial<BackendClient>): BackendClient {
  return {
    mode: "desktop",
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    alerting: {} as BackendClient["alerting"],
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
