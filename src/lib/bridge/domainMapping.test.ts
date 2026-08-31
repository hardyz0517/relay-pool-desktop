import { describe, expect, it } from "vitest";
import { normalizeSettings } from "./domainMapping";
import type { AppSettings } from "@/lib/types/settings";

describe("normalizeSettings", () => {
  it("recovers the five-minute published-status default when an older backend omits the field", () => {
    const olderSettings = {
      localProxyPort: 8787,
      localProxyStartOnLaunch: false,
      localKeyMasked: "sk-local-fixture",
      collectorProxyMode: "direct",
      collectorProxyUrl: null,
      lowBalanceThresholdCny: 15,
      collectorIntervalMinutes: 30,
      balanceIntervalMinutes: 5,
      groupRateIntervalMinutes: 20,
      pricingRefreshIntervalMinutes: 60,
      collectorTimeoutSeconds: 15,
      collectorMaxConcurrency: 3,
      developerModeEnabled: false,
      dataDir: "fixture-data-dir",
      pendingDataDir: null,
      dataDirChangeRequiresRestart: false,
    } as unknown as AppSettings;

    expect(normalizeSettings(olderSettings).publishedStatusIntervalMinutes).toBe(5);
  });
});
