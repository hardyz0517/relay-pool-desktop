import { describe, expect, it } from "vitest";
import {
  applyCollectorFrequencyPreset,
  createCollectorSettingsDraft,
  createRecommendedCollectorSettingsDraft,
  detectCollectorFrequencyPreset,
  parseCollectorSettingsDraft,
  type CollectorSettingsDraft,
} from "./collectorSettingsForm";
import type { AppSettings } from "@/lib/types/settings";

function settings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    localProxyPort: 8787,
    localProxyStartOnLaunch: false,
    localKeyMasked: "sk-local-fixture",
    defaultRoutingStrategy: "automatic_balanced",
    collectorProxyMode: "direct",
    collectorProxyUrl: null,
    maxRateMultiplier: null,
    defaultRoutingGroupFilter: "all_groups",
    schedulerAdvancedSettings: {
      topK: 7,
      multiplier: 1,
      priority: 1,
      load: 1,
      queue: 0.7,
      errorRate: 0.8,
      ttft: 0.5,
      quotaHeadroom: 0,
      previousResponse: 5,
      sessionSticky: 3,
      multiplierMinConfidence: 0.8,
      stickyWeighted: false,
      stickyEscape: true,
      stickyEscapeTtftMs: 15_000,
      stickyEscapeErrorRate: 0.5,
      stickySessionTtlSeconds: 3_600,
      stickyResponseTtlSeconds: 3_600,
      stickyMaxWaiting: 3,
      stickyWaitTimeoutSeconds: 120,
      fallbackMaxWaiting: 100,
      fallbackWaitTimeoutSeconds: 30,
    },
    lowBalanceThresholdCny: 15,
    collectorIntervalMinutes: 30,
    balanceIntervalMinutes: 5,
    groupRateIntervalMinutes: 20,
    publishedStatusIntervalMinutes: 5,
    pricingRefreshIntervalMinutes: 60,
    collectorTimeoutSeconds: 15,
    collectorMaxConcurrency: 3,
    allowDepletedFallback: false,
    developerModeEnabled: false,
    dataDir: "fixture-data-dir",
    pendingDataDir: null,
    dataDirChangeRequiresRestart: false,
    ...overrides,
  };
}

function draft(overrides: Partial<CollectorSettingsDraft> = {}): CollectorSettingsDraft {
  return {
    balanceIntervalMinutes: "5",
    groupRateIntervalMinutes: "20",
    publishedStatusIntervalMinutes: "5",
    pricingRefreshIntervalMinutes: "60",
    collectorTimeoutSeconds: "15",
    collectorMaxConcurrency: "3",
    ...overrides,
  };
}

describe("collector settings form", () => {
  it("round-trips the independent published-status interval from persisted settings", () => {
    expect(createCollectorSettingsDraft(settings({ publishedStatusIntervalMinutes: 1440 })))
      .toMatchObject({ publishedStatusIntervalMinutes: "1440" });
  });

  it.each([
    ["1", 1],
    ["1440", 1440],
  ])("accepts the published-status interval boundary %s", (rawValue, expected) => {
    const result = parseCollectorSettingsDraft(draft({ publishedStatusIntervalMinutes: rawValue }));

    expect(result).toEqual({
      ok: true,
      value: expect.objectContaining({ publishedStatusIntervalMinutes: expected }),
    });
  });

  it.each(["0", "1441", "1.5", "", "not-a-number"])("rejects invalid published-status interval %j without dropping the error", (rawValue) => {
      const result = parseCollectorSettingsDraft(draft({ publishedStatusIntervalMinutes: rawValue }));

      expect(result).toEqual({
        ok: false,
        errors: {
          publishedStatusIntervalMinutes: "请输入 1 到 1440 的整数",
        },
      });
    });

  it.each([
    ["timely", "2"],
    ["balanced", "5"],
    ["resource_saver", "15"],
  ] as const)("applies the %s preset including the independent published-status cadence", (preset, expectedInterval) => {
    const applied = applyCollectorFrequencyPreset(
      draft({ collectorTimeoutSeconds: "22", collectorMaxConcurrency: "6" }),
      preset,
    );

    expect(applied.publishedStatusIntervalMinutes).toBe(expectedInterval);
    expect(applied.collectorTimeoutSeconds).toBe("22");
    expect(applied.collectorMaxConcurrency).toBe("6");
    expect(detectCollectorFrequencyPreset(applied)).toBe(preset);
  });

  it("treats a published-status-only cadence change as custom", () => {
    expect(detectCollectorFrequencyPreset(draft({ publishedStatusIntervalMinutes: "10" }))).toBe("custom");
  });

  it("restores the recommended preset with the published-status default and valid execution settings", () => {
    const recommended = createRecommendedCollectorSettingsDraft();

    expect(recommended).toEqual({
      balanceIntervalMinutes: "5",
      groupRateIntervalMinutes: "20",
      publishedStatusIntervalMinutes: "5",
      pricingRefreshIntervalMinutes: "60",
      collectorTimeoutSeconds: "15",
      collectorMaxConcurrency: "3",
    });
    expect(parseCollectorSettingsDraft(recommended)).toEqual({
      ok: true,
      value: {
        balanceIntervalMinutes: 5,
        groupRateIntervalMinutes: 20,
        publishedStatusIntervalMinutes: 5,
        pricingRefreshIntervalMinutes: 60,
        collectorTimeoutSeconds: 15,
        collectorMaxConcurrency: 3,
      },
    });
  });
});
