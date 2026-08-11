import { describe, expect, it } from "vitest";

import { effectiveRateMultiplierForCredit } from "./formatters";

describe("effectiveRateMultiplierForCredit", () => {
  it("normalizes a station-native multiplier by the exchange rate", () => {
    expect(effectiveRateMultiplierForCredit(2, 27)).toBeCloseTo(2 / 27);
  });

  it("uses the neutral exchange rate when the configured rate is invalid", () => {
    expect(effectiveRateMultiplierForCredit(0.5, 0)).toBe(0.5);
    expect(effectiveRateMultiplierForCredit(0.5, Number.NaN)).toBe(0.5);
  });

  it("returns null when the source multiplier is missing or invalid", () => {
    expect(effectiveRateMultiplierForCredit(null, 1)).toBeNull();
    expect(effectiveRateMultiplierForCredit(Number.NaN, 1)).toBeNull();
  });
});
