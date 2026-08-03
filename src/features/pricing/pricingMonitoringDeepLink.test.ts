import { describe, expect, it } from "vitest";
import { buildPricingMonitoringDeepLink } from "./pricingMonitoringDeepLink";

describe("buildPricingMonitoringDeepLink", () => {
  it("uses only the representative key id when one exists", () => {
    expect(
      buildPricingMonitoringDeepLink({
        stationId: "station-1",
        monitorSummary: {
          representativeKeyId: "key-1",
        } as never,
      }),
    ).toEqual({
      kind: "station-key",
      stationKeyId: "key-1",
      source: "pricing",
    });
  });

  it("falls back to a station-only link without fabricating a monitor or key id", () => {
    expect(
      buildPricingMonitoringDeepLink({
        stationId: "station-1",
        monitorSummary: null,
      }),
    ).toEqual({
      kind: "station",
      stationId: "station-1",
      source: "pricing",
    });
  });

  it("never puts credential or URL fields into the link", () => {
    const link = buildPricingMonitoringDeepLink({
      stationId: "station-1",
      monitorSummary: {
        representativeKeyId: "key-1",
        latestTerminalReason: "redacted",
      } as never,
    });
    expect(JSON.stringify(link)).not.toMatch(/apiKey|cookie|token|https?:\/\//i);
  });
});
