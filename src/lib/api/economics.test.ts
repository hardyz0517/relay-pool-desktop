import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  deletePricingRule: vi.fn(),
  listBalanceSnapshots: vi.fn(),
  listBalanceSnapshotsForStation: vi.fn(),
  listCurrentStationBalanceSnapshots: vi.fn(),
  listModelBasePrices: vi.fn(),
  listPricingRules: vi.fn(),
  resetModelBasePricesToBuiltins: vi.fn(),
  resolveStationKeyPricingContext: vi.fn(),
  upsertBalanceSnapshot: vi.fn(),
  upsertModelBasePrice: vi.fn(),
  upsertPricingRule: vi.fn(),
}));

const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import {
  deletePricingRule,
  resetModelBasePricesToBuiltins,
  upsertModelBasePrice,
  upsertPricingRule,
} from "./economics";

describe("pricing mutation generated transport cutover", () => {
  beforeEach(() => {
    for (const fn of Object.values(generated)) fn.mockReset().mockResolvedValue(undefined);
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
  });

  it("routes all four pricing mutations through generated wrappers", async () => {
    const basePrice = {
      id: null,
      provider: "openai",
      model: "fixture-model",
      inputPrice: 1,
      outputPrice: 2,
      currency: "USD",
      unit: "M",
      sourceUrl: "https://example.test/pricing",
      sourceLabel: "Fixture",
      sourceCheckedAt: null,
      enabled: true,
      builtIn: false,
      note: null,
    };
    const rule = {
      id: null,
      stationId: "station-1",
      stationKeyId: null,
      groupBindingId: null,
      groupName: null,
      tierLabel: null,
      model: "fixture-model",
      inputPrice: 1,
      outputPrice: 2,
      fixedPrice: null,
      rateMultiplier: 1,
      currency: "USD",
      unit: "M",
      priceType: "token",
      basePriceSource: null,
      normalizationStatus: null,
      source: "manual",
      confidence: 1,
      enabled: true,
      note: null,
      collectedAt: null,
      validFrom: null,
      validUntil: null,
    };

    await upsertModelBasePrice(basePrice);
    await resetModelBasePricesToBuiltins();
    await upsertPricingRule(rule);
    await deletePricingRule("rule-1");

    expect(generated.upsertModelBasePrice).toHaveBeenCalledWith(basePrice);
    expect(generated.resetModelBasePricesToBuiltins).toHaveBeenCalledWith();
    expect(generated.upsertPricingRule).toHaveBeenCalledWith(rule);
    expect(generated.deletePricingRule).toHaveBeenCalledWith({ id: "rule-1" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});
