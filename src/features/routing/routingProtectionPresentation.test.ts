import { describe, expect, it } from "vitest";
import type { RoutingProtectionStatus } from "@/lib/types/routing";
import { userVisibleProtectionEntries } from "./routingProtectionPresentation";

function statusWithEntries(entries: RoutingProtectionStatus["entries"]): RoutingProtectionStatus {
  return {
    statusVersion: "routing_protection_status_v1",
    generatedAtMs: 1,
    entries,
    readModelStatus: "available",
    timeouts: null,
  };
}

describe("routing protection presentation", () => {
  it("hides compatibility snapshots while retaining effective protection facts", () => {
    const visible = userVisibleProtectionEntries(statusWithEntries([
      {
        scope: "legacy_station_key:v1:hash",
        scopeKind: "legacy_station_key",
        state: "degraded",
        explanationKey: "routing.protection.legacy_degraded",
        persistenceKind: "legacy_compatibility",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: "legacy_failure",
        updatedAtMs: 1,
        detailAvailable: true,
      },
      {
        scope: "credential:hash",
        scopeKind: "credential",
        state: "cooldown",
        explanationKey: "routing.protection.cooldown",
        persistenceKind: "durable",
        cooldownUntilMs: 2_000,
        cooldownRemainingMs: 1_000,
        recentFailureCode: "upstream_overloaded",
        updatedAtMs: 1,
        detailAvailable: true,
      },
      {
        scope: "capacity:hash",
        scopeKind: "capacity_domain",
        state: "half_open",
        explanationKey: "routing.protection.half_open",
        persistenceKind: "runtime_capacity",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: "capacity_exhausted",
        updatedAtMs: 1,
        detailAvailable: true,
      },
    ]));

    expect(visible.map((entry) => entry.persistenceKind)).toEqual([
      "durable",
      "runtime_capacity",
    ]);
    expect(visible.some((entry) => entry.scope.includes("legacy_station_key"))).toBe(false);
  });

  it("returns an empty list when the protection read model is unavailable", () => {
    expect(userVisibleProtectionEntries(null)).toEqual([]);
    expect(userVisibleProtectionEntries(undefined)).toEqual([]);
  });

  it("hides unavailable and empty protection placeholders", () => {
    const visible = userVisibleProtectionEntries(statusWithEntries([
      {
        scope: "runtime_capacity",
        scopeKind: "capacity_domain",
        state: "unavailable",
        explanationKey: "routing.protection.unavailable",
        persistenceKind: "runtime_capacity",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: null,
        updatedAtMs: null,
        detailAvailable: false,
      },
      {
        scope: "routing",
        scopeKind: null,
        state: "no_protection",
        explanationKey: "routing.protection.none_active",
        persistenceKind: null,
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: null,
        updatedAtMs: 1,
        detailAvailable: true,
      },
    ]));

    expect(visible).toEqual([]);
  });
});
