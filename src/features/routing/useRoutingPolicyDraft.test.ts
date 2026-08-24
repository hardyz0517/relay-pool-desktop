import { describe, expect, it } from "vitest";
import type { RoutingPolicySnapshot } from "@/lib/types/routing";
import {
  createDefaultRoutingPolicyConfig,
  initialRoutingPolicyDraftState,
  routingPolicyDraftFieldHints,
  routingPolicyDraftIsDirty,
  routingPolicyDraftReducer,
} from "./useRoutingPolicyDraft";

function snapshot(revision: number, overrides: Record<string, unknown> = {}): RoutingPolicySnapshot {
  return {
    config: {
      version: 2,
      reliabilityWeight: 4_000,
      responsivenessWeight: 2_500,
      costWeight: 2_000,
      preferenceWeight: 1_500,
      maxCandidates: 64,
      explorationShareBasisPoints: 500,
      allowDepletedFallback: false,
      affinityEnabled: false,
      affinityTtlSeconds: 300,
      maxRateMultiplier: null,
      routingGroupFilter: "all_groups",
      outboundProxyMode: "inherit",
      outboundProxyUrl: null,
      retryFailover: {
        version: 2,
        maxTotalAttempts: 4,
        maxSameTargetCapacityRetries: 2,
        capacityRetryWaitBudgetSeconds: 2,
        allowCrossCapacityDomainFallback: true,
      },
      protectionProfile: {
        version: 2,
        enabled: false,
        windowMaxSamples: 64,
        windowSeconds: 300,
        minSamples: 5,
        failureThresholdPercent: 60,
        halfOpenSuccessesToClose: 2,
      },
      timeoutPolicy: {
        version: 2,
        connectSeconds: 10,
        firstByteSeconds: 30,
        precommitSeconds: 60,
        bufferedExecutionSeconds: 300,
        streamIdleSeconds: 90,
      },
      ...overrides,
    },
    revision,
    policyVersion: "routing-policy-v2",
    systemVersion: "routing-system-v1",
    status: "active",
    updatedAtMs: revision,
    documentSync: null,
  };
}

describe("routing policy draft reducer", () => {
  it("provides non-blocking hints for invalid retry combinations", () => {
    const config = snapshot(1).config;
    config.retryFailover.maxTotalAttempts = 2;
    config.retryFailover.maxSameTargetCapacityRetries = 2;
    expect(routingPolicyDraftFieldHints(config)["retryFailover.maxSameTargetCapacityRetries"]).toContain("必须小于");
    config.retryFailover.maxSameTargetCapacityRetries = 1;
    expect(routingPolicyDraftFieldHints(config)).toEqual({});
  });

  it("uses the baseline V2 defaults for a reversible reset", () => {
    const defaults = createDefaultRoutingPolicyConfig();
    expect(defaults.version).toBe(2);
    expect(defaults.maxCandidates).toBe(64);
    expect(defaults.retryFailover).toEqual({
      version: 2,
      maxTotalAttempts: 4,
      maxSameTargetCapacityRetries: 2,
      capacityRetryWaitBudgetSeconds: 2,
      allowCrossCapacityDomainFallback: true,
    });
  });

  it("hydrates a clean draft and keeps a local edit dirty", () => {
    const hydrated = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3),
    });
    expect(hydrated.baseRevision).toBe(3);
    const edited = routingPolicyDraftReducer(hydrated, {
      type: "edit",
      config: { ...hydrated.config!, maxCandidates: 32 },
    });
    expect(routingPolicyDraftIsDirty(edited)).toBe(true);
    expect(edited.status).toBe("dirty");
  });

  it("turns an external revision into a typed conflict without dropping local edits", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3),
    });
    const edited = routingPolicyDraftReducer(clean, {
      type: "edit",
      config: { ...clean.config!, retryFailover: { ...clean.config!.retryFailover, maxTotalAttempts: 3 } },
    });
    const conflicted = routingPolicyDraftReducer(edited, {
      type: "hydrate",
      snapshot: snapshot(4, { maxCandidates: 128 }),
    });
    expect(conflicted.status).toBe("conflict");
    expect(conflicted.config?.retryFailover.maxTotalAttempts).toBe(3);
    expect(conflicted.remoteSnapshot?.revision).toBe(4);
  });

  it("does not regress after a stale query response arrives", () => {
    const current = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(5),
    });
    const saved = routingPolicyDraftReducer(current, {
      type: "saveSuccess",
      snapshot: snapshot(6),
    });
    const stale = routingPolicyDraftReducer(saved, {
      type: "hydrate",
      snapshot: snapshot(5, { maxCandidates: 999 }),
    });
    expect(stale.baseRevision).toBe(6);
    expect(stale.config?.maxCandidates).toBe(64);
  });

  it("merges only locally changed fields from the latest remote document", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3),
    });
    const edited = routingPolicyDraftReducer(clean, {
      type: "edit",
      config: { ...clean.config!, maxCandidates: 32 },
    });
    const conflicted = routingPolicyDraftReducer(edited, {
      type: "hydrate",
      snapshot: snapshot(4, { maxCandidates: 128, explorationShareBasisPoints: 900 }),
    });
    const merged = routingPolicyDraftReducer(conflicted, { type: "mergeRemote" });
    expect(merged.status).toBe("dirty");
    expect(merged.baseRevision).toBe(4);
    expect(merged.config?.maxCandidates).toBe(32);
    expect(merged.config?.explorationShareBasisPoints).toBe(900);
  });

  it("merges retryFailover fields independently during a conflict", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3),
    });
    const edited = routingPolicyDraftReducer(clean, {
      type: "edit",
      config: {
        ...clean.config!,
        retryFailover: {
          ...clean.config!.retryFailover,
          maxTotalAttempts: 3,
        },
      },
    });
    const conflicted = routingPolicyDraftReducer(edited, {
      type: "hydrate",
      snapshot: snapshot(4, {
        retryFailover: {
          ...clean.config!.retryFailover,
          capacityRetryWaitBudgetSeconds: 1,
        },
      }),
    });
    const merged = routingPolicyDraftReducer(conflicted, { type: "mergeRemote" });
    expect(merged.config?.retryFailover.maxTotalAttempts).toBe(3);
    expect(merged.config?.retryFailover.capacityRetryWaitBudgetSeconds).toBe(1);
  });

  it("discard and overwrite are explicit choices", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3),
    });
    const edited = routingPolicyDraftReducer(clean, {
      type: "edit",
      config: { ...clean.config!, maxCandidates: 32 },
    });
    const discarded = routingPolicyDraftReducer(edited, { type: "discard" });
    expect(discarded.config?.maxCandidates).toBe(64);
    const remote = routingPolicyDraftReducer(edited, {
      type: "hydrate",
      snapshot: snapshot(4, { maxCandidates: 128 }),
    });
    const overwritten = routingPolicyDraftReducer(remote, { type: "overwriteRemote" });
    expect(overwritten.baseRevision).toBe(4);
    expect(overwritten.config?.maxCandidates).toBe(32);
    expect(overwritten.status).toBe("dirty");
  });

  it("retains field-addressable save errors until the next draft edit", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3),
    });
    const error = routingPolicyDraftReducer(clean, {
      type: "saveError",
      error: "策略验证失败",
      fieldErrors: { "retryFailover.maxTotalAttempts": "必须在范围内" },
    });
    expect(error.status).toBe("error");
    expect(error.fieldErrors["retryFailover.maxTotalAttempts"]).toBe("必须在范围内");
    const edited = routingPolicyDraftReducer(error, {
      type: "edit",
      config: { ...error.config!, maxCandidates: 32 },
    });
    expect(edited.fieldErrors).toEqual({});
  });
});
