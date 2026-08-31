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
  it("shows only active station-key protection facts", () => {
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
        diagnosticReason: null,
        updatedAtMs: 1,
        detailAvailable: true,
      },
      {
        scope: "station_key:key-1",
        scopeKind: "station_key",
        state: "cooldown",
        explanationKey: "routing.protection.cooldown",
        persistenceKind: "durable",
        cooldownUntilMs: 2_000,
        cooldownRemainingMs: 1_000,
        recentFailureCode: "upstream_overloaded",
        diagnosticReason: null,
        updatedAtMs: 1,
        detailAvailable: true,
      },
      {
        scope: "local-capacity:hash",
        scopeKind: "local_capacity",
        state: "half_open",
        explanationKey: "routing.protection.half_open",
        persistenceKind: "runtime_capacity",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: "capacity_exhausted",
        diagnosticReason: "capacity_exhausted",
        updatedAtMs: 1,
        detailAvailable: true,
      },
      {
        scope: "credential:hash",
        scopeKind: "credential",
        state: "open",
        explanationKey: "routing.protection.open",
        persistenceKind: "durable",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: "upstream_failure",
        diagnosticReason: null,
        updatedAtMs: 1,
        detailAvailable: true,
      },
    ]));

    expect(visible.map((entry) => entry.scope)).toEqual(["station_key:key-1"]);
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
        scopeKind: "local_capacity",
        state: "unavailable",
        explanationKey: "routing.protection.unavailable",
        persistenceKind: "runtime_capacity",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: null,
        diagnosticReason: "capacity_state_unavailable",
        updatedAtMs: null,
        detailAvailable: false,
      },
      {
        scope: "station_key:key-1",
        scopeKind: "station_key",
        state: "unavailable",
        explanationKey: "routing.protection.unavailable",
        persistenceKind: "durable",
        cooldownUntilMs: null,
        cooldownRemainingMs: null,
        recentFailureCode: null,
        diagnosticReason: null,
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
        diagnosticReason: null,
        updatedAtMs: 1,
        detailAvailable: true,
      },
    ]));

    expect(visible).toEqual([]);
  });
});
