import { describe, expect, it } from "vitest";
import type { AppSettings, RoutingStrategy } from "@/lib/types/settings";
import {
  createRoutingMigrationDraft,
  evaluateRoutingMigrationReadiness,
  proposedRoutingOrderingProfile,
} from "./routingMigrationReadiness";

function settings(policy: RoutingStrategy, overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    localProxyPort: 8787,
    localProxyStartOnLaunch: false,
    localKeyMasked: "sk-...redacted",
    defaultRoutingStrategy: policy,
    collectorProxyMode: "direct",
    collectorProxyUrl: null,
    maxRateMultiplier: 2,
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
    modelListIntervalMinutes: 60,
    pricingRefreshIntervalMinutes: 60,
    collectorTimeoutSeconds: 15,
    collectorMaxConcurrency: 3,
    allowDepletedFallback: false,
    hierarchicalRoutingMigration: null,
    developerModeEnabled: false,
    dataDir: "fixture",
    pendingDataDir: null,
    dataDirChangeRequiresRestart: false,
    ...overrides,
  };
}

describe("routing migration readiness", () => {
  it.each([
    ["priority_fallback", "priority_first"],
    ["stable_first", "priority_first"],
    ["cheap_first", "cost_first"],
    ["cost_stable_first", "cost_first"],
  ] as const)("maps %s to %s", (legacy, profile) => {
    expect(proposedRoutingOrderingProfile(legacy)).toBe(profile);
  });

  it("requires manual policy choice for backup-only and automatic policies", () => {
    expect(proposedRoutingOrderingProfile("backup_only")).toBeNull();
    expect(proposedRoutingOrderingProfile("automatic_balanced")).toBeNull();
  });

  it("is not ready until every migration dimension is explicitly confirmed", () => {
    const current = settings("cost_stable_first");
    const draft = createRoutingMigrationDraft(current);

    const readiness = evaluateRoutingMigrationReadiness(current, draft);

    expect(readiness.ready).toBe(false);
    expect(readiness.issues).toEqual([
      "group_scope_unconfirmed",
      "backup_depleted_unconfirmed",
      "affinity_unconfirmed",
    ]);
    expect(readiness.input).toBeNull();
  });

  it("produces one complete hierarchical config input when ready", () => {
    const current = settings("stable_first");
    const draft = {
      ...createRoutingMigrationDraft(current),
      groupScopeConfirmed: true,
      backupDepletedConfirmed: true,
      affinityMode: "session" as const,
    };

    const readiness = evaluateRoutingMigrationReadiness(current, draft);

    expect(readiness.ready).toBe(true);
    expect(readiness.input).toEqual({
      orderingProfile: "priority_first",
      multiplierCeiling: 2,
      groupScope: "all_groups",
      allowDepletedFallback: false,
      affinityMode: "session",
      legacyPolicy: "stable_first",
    });
  });
});
