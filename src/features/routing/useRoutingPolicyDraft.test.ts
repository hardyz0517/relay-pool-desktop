import { describe, expect, it } from "vitest";
import type {
  RoutingPolicyPublicationStatus,
  RoutingPolicySnapshot,
} from "@/lib/types/routing";
import {
  createDefaultRoutingPolicyConfig,
  initialRoutingPolicyDraftState,
  pollRoutingPolicyPublication,
  routingPolicyDraftFieldHints,
  routingPolicyDraftIsDirty,
  routingPolicyDraftReducer,
} from "./useRoutingPolicyDraft";

function publication(
  status: RoutingPolicyPublicationStatus["status"],
  policyGenerationId: string | null = "pg1_fixture",
  failureCode: RoutingPolicyPublicationStatus["failureCode"] = null,
): RoutingPolicyPublicationStatus {
  return {
    revision: 4,
    policyGenerationId,
    status,
    failureCode,
    updatedAtMs: 10,
    terminal: status === "active" || status === "failed" || status === "expired",
  };
}

function snapshot(
  revision: number,
  overrides: Record<string, unknown> = {},
  status = "active",
): RoutingPolicySnapshot {
  return {
    config: { ...createDefaultRoutingPolicyConfig(), ...overrides },
    revision,
    policyVersion: "routing-policy-v3",
    systemVersion: "routing-system-v1",
    status,
    updatedAtMs: revision,
    documentSync: null,
  };
}

describe("routing policy draft reducer", () => {
  it("uses the V3 defaults and removes retired user controls", () => {
    const defaults = createDefaultRoutingPolicyConfig();
    expect(defaults.version).toBe(3);
    expect(defaults.reliabilitySourceWeights).toEqual({ realTrafficPercent: 70, monitoringPercent: 30 });
    expect(defaults.reliabilitySampling).toEqual({
      historicalMinimumSamples: 15,
      recentMinimumSamples: 5,
      optimisticReliabilityPercent: 95,
      optimisticLatencyMs: 2_500,
    });
    expect(defaults.retry).toEqual({ version: 1, maxRetryCount: 3, consecutiveFailureThreshold: 3 });
    expect(defaults.circuitBreaker).toEqual({ version: 1, recoverySuccessThreshold: 2, recoveryWaitSeconds: 30 });
    expect(defaults).not.toHaveProperty("maxCandidates");
    expect(defaults).not.toHaveProperty("explorationShareBasisPoints");
    expect(defaults).not.toHaveProperty("retryFailover");
    expect(defaults).not.toHaveProperty("protectionProfile");
  });

  it("validates source weights without changing the user's values", () => {
    const config = createDefaultRoutingPolicyConfig();
    config.reliabilitySourceWeights = { realTrafficPercent: 80, monitoringPercent: 10 };
    expect(routingPolicyDraftFieldHints(config).reliabilitySourceWeights).toContain("100");
    config.reliabilitySourceWeights = { realTrafficPercent: 70, monitoringPercent: 30 };
    expect(routingPolicyDraftFieldHints(config)).toEqual({});
    config.reliabilitySourceWeights = { realTrafficPercent: 70.5, monitoringPercent: 29.5 };
    expect(routingPolicyDraftFieldHints(config).reliabilitySourceWeights).toContain("整数");
    expect(config.reliabilitySourceWeights).toEqual({ realTrafficPercent: 70.5, monitoringPercent: 29.5 });
  });

  it("hydrates a clean V3 draft and keeps edits dirty", () => {
    const hydrated = routingPolicyDraftReducer(initialRoutingPolicyDraftState, { type: "hydrate", snapshot: snapshot(3) });
    expect(hydrated.baseRevision).toBe(3);
    const edited = routingPolicyDraftReducer(hydrated, {
      type: "edit",
      config: { ...hydrated.config!, retry: { ...hydrated.config!.retry, maxRetryCount: 1 } },
    });
    expect(routingPolicyDraftIsDirty(edited)).toBe(true);
    expect(edited.status).toBe("dirty");
  });

  it("tracks staged publication progress and ignores edits while saving", () => {
    const staged = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3, {}, "staged"),
    });
    expect(staged.publicationStatus).toBe("staged");

    const saving = routingPolicyDraftReducer(staged, { type: "saveStart" });
    const ignoredEdit = routingPolicyDraftReducer(saving, {
      type: "edit",
      config: { ...saving.config!, reliabilityWeight: 9_000 },
    });
    expect(ignoredEdit).toBe(saving);

    const ready = routingPolicyDraftReducer(saving, {
      type: "hydrate",
      snapshot: snapshot(3, {}, "ready"),
    });
    expect(ready.publicationStatus).toBe("ready");
    expect(ready.config?.reliabilityWeight).toBe(4_000);
  });

  it("starts publication polling after a staged save and retains the generation fence", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
      type: "hydrate",
      snapshot: snapshot(3),
    });
    const staged = routingPolicyDraftReducer(clean, {
      type: "saveSuccess",
      snapshot: snapshot(4, {}, "staged"),
      publicationStartedAtMs: 1_000,
    });
    expect(staged.publicationPollingState).toBe("polling");
    expect(staged.publicationStartedAtMs).toBe(1_000);

    const ready = routingPolicyDraftReducer(staged, {
      type: "publicationUpdate",
      publication: publication("ready"),
    });
    expect(ready.publicationGenerationId).toBe("pg1_fixture");
    expect(ready.publicationStatus).toBe("ready");
    expect(ready.publicationStartedAtMs).toBe(1_000);

    const active = routingPolicyDraftReducer(ready, {
      type: "publicationUpdate",
      publication: publication("active"),
    });
    expect(active.publicationStatus).toBe("active");
    expect(active.publicationPollingState).toBe("idle");
    expect(active.publicationStartedAtMs).toBeNull();
  });

  it("keeps an unavailable publication non-active and stops after a bounded timeout", () => {
    const staged = routingPolicyDraftReducer(
      routingPolicyDraftReducer(initialRoutingPolicyDraftState, {
        type: "hydrate",
        snapshot: snapshot(3),
      }),
      {
        type: "saveSuccess",
        snapshot: snapshot(4, {}, "staged"),
        publicationStartedAtMs: 1_000,
      },
    );
    const unavailable = routingPolicyDraftReducer(staged, {
      type: "publicationUnavailable",
      revision: 4,
    });
    expect(unavailable.publicationStatus).toBe("staged");
    expect(unavailable.publicationPollingState).toBe("unavailable");
    expect(unavailable.publicationError).toContain("尚未确认");

    const timedOut = routingPolicyDraftReducer(unavailable, {
      type: "publicationTimeout",
      revision: 4,
    });
    expect(timedOut.publicationStatus).toBe("expired");
    expect(timedOut.publicationPollingState).toBe("timed_out");
    expect(timedOut.publicationStartedAtMs).toBeNull();
  });

  it("turns an external revision into a conflict without dropping local edits", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, { type: "hydrate", snapshot: snapshot(3) });
    const edited = routingPolicyDraftReducer(clean, {
      type: "edit",
      config: { ...clean.config!, retry: { ...clean.config!.retry, maxRetryCount: 1 } },
    });
    const conflicted = routingPolicyDraftReducer(edited, {
      type: "hydrate",
      snapshot: snapshot(4, { reliabilityWeight: 5_000 }),
    });
    expect(conflicted.status).toBe("conflict");
    expect(conflicted.config?.retry.maxRetryCount).toBe(1);
    expect(conflicted.remoteSnapshot?.revision).toBe(4);
  });

  it("merges nested V3 fields independently", () => {
    const clean = routingPolicyDraftReducer(initialRoutingPolicyDraftState, { type: "hydrate", snapshot: snapshot(3) });
    const edited = routingPolicyDraftReducer(clean, {
      type: "edit",
      config: {
        ...clean.config!,
        retry: { ...clean.config!.retry, maxRetryCount: 1 },
        reliabilitySampling: { ...clean.config!.reliabilitySampling, recentMinimumSamples: 8 },
      },
    });
    const conflicted = routingPolicyDraftReducer(edited, {
      type: "hydrate",
      snapshot: snapshot(4, {
        retry: { ...clean.config!.retry, consecutiveFailureThreshold: 5 },
        reliabilitySampling: { ...clean.config!.reliabilitySampling, historicalMinimumSamples: 20 },
      }),
    });
    const merged = routingPolicyDraftReducer(conflicted, { type: "mergeRemote" });
    expect(merged.config?.retry.maxRetryCount).toBe(1);
    expect(merged.config?.retry.consecutiveFailureThreshold).toBe(5);
    expect(merged.config?.reliabilitySampling.recentMinimumSamples).toBe(8);
    expect(merged.config?.reliabilitySampling.historicalMinimumSamples).toBe(20);
  });

  it("does not regress after a stale query response and supports discard/overwrite", () => {
    const current = routingPolicyDraftReducer(initialRoutingPolicyDraftState, { type: "hydrate", snapshot: snapshot(5) });
    const saved = routingPolicyDraftReducer(current, { type: "saveSuccess", snapshot: snapshot(6) });
    const stale = routingPolicyDraftReducer(saved, { type: "hydrate", snapshot: snapshot(5, { reliabilityWeight: 9_000 }) });
    expect(stale.baseRevision).toBe(6);
    const edited = routingPolicyDraftReducer(saved, {
      type: "edit",
      config: { ...saved.config!, circuitBreaker: { ...saved.config!.circuitBreaker, recoveryWaitSeconds: 60 } },
    });
    const remote = routingPolicyDraftReducer(edited, { type: "hydrate", snapshot: snapshot(7) });
    const overwritten = routingPolicyDraftReducer(remote, { type: "overwriteRemote" });
    expect(overwritten.baseRevision).toBe(7);
    expect(overwritten.config?.circuitBreaker.recoveryWaitSeconds).toBe(60);
    expect(routingPolicyDraftReducer(edited, { type: "discard" }).config).toEqual(saved.config);
  });
});

describe("routing policy publication polling", () => {
  it("fences later polls with the first generation id and keeps polling through ready", async () => {
    const inputs: Array<{ revision: number; policyGenerationId?: string | null }> = [];
    const statuses = [publication("staged"), publication("ready"), publication("active")];
    const observed: string[] = [];
    const outcome = await pollRoutingPolicyPublication({
      revision: 4,
      startedAtMs: 0,
      signal: new AbortController().signal,
      now: () => 0,
      wait: async () => undefined,
      fetchStatus: async (input) => {
        inputs.push(input);
        return statuses.shift()!;
      },
      onStatus: (status) => observed.push(status.status),
      onUnavailable: () => undefined,
    });

    expect(outcome).toBe("terminal");
    expect(observed).toEqual(["staged", "ready", "active"]);
    expect(inputs).toEqual([
      { revision: 4, policyGenerationId: null },
      { revision: 4, policyGenerationId: "pg1_fixture" },
      { revision: 4, policyGenerationId: "pg1_fixture" },
    ]);
  });

  it.each(["failed", "expired"] as const)("stops on %s", async (terminalStatus) => {
    let calls = 0;
    const outcome = await pollRoutingPolicyPublication({
      revision: 4,
      startedAtMs: 0,
      signal: new AbortController().signal,
      now: () => 0,
      wait: async () => undefined,
      fetchStatus: async () => {
        calls += 1;
        return publication(terminalStatus);
      },
      onStatus: () => undefined,
      onUnavailable: () => undefined,
    });
    expect(outcome).toBe("terminal");
    expect(calls).toBe(1);
  });

  it("reports transient read errors without claiming activation and can recover", async () => {
    let calls = 0;
    let unavailable = 0;
    const observed: string[] = [];
    const outcome = await pollRoutingPolicyPublication({
      revision: 4,
      startedAtMs: 0,
      signal: new AbortController().signal,
      now: () => 0,
      wait: async () => undefined,
      fetchStatus: async () => {
        calls += 1;
        if (calls === 1) throw new Error("unavailable");
        return publication("active");
      },
      onStatus: (status) => observed.push(status.status),
      onUnavailable: () => {
        unavailable += 1;
      },
    });
    expect(outcome).toBe("terminal");
    expect(unavailable).toBe(1);
    expect(observed).toEqual(["active"]);
  });

  it("terminates at the polling deadline", async () => {
    let nowMs = 0;
    let calls = 0;
    const outcome = await pollRoutingPolicyPublication({
      revision: 4,
      startedAtMs: 0,
      signal: new AbortController().signal,
      intervalMs: 10,
      timeoutMs: 25,
      now: () => nowMs,
      wait: async (delayMs) => {
        nowMs += delayMs;
      },
      fetchStatus: async () => {
        calls += 1;
        return publication("staged");
      },
      onStatus: () => undefined,
      onUnavailable: () => undefined,
    });
    expect(outcome).toBe("timed_out");
    expect(calls).toBe(3);
  });
});
