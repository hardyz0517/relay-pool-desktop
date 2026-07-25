import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";

import { reorderLocalRoutingKeys } from "./localRouting";

describe("local routing backend cutover", () => {
  const localRouting = {
    loadLocalRoutingWorkspace: vi.fn(async () => workspace()),
    reorderLocalRoutingKeys: vi.fn(async () => workspace()),
  };

  beforeEach(() => {
    setActiveBackendClient({
      mode: "desktop",
      settings: {} as never,
      stations: {} as never,
      stationKeys: {} as never,
      changeEvents: {} as never,
      collectorRuns: {} as never,
      proxy: {} as never,
      localRouting: localRouting as never,
      dataRecovery: {} as never,
      economics: {} as never,
      groupFacts: {} as never,
      pricing: {} as never,
      handshake: vi.fn(async () => ({}) as never),
    });
    localRouting.loadLocalRoutingWorkspace.mockReset();
    localRouting.reorderLocalRoutingKeys.mockReset();
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes reorder through the active backend client", async () => {
    const input = { stationKeyIds: ["key-1", "key-2"] };
    await reorderLocalRoutingKeys(input);

    expect(localRouting.reorderLocalRoutingKeys).toHaveBeenCalledWith(input);
  });
});

function workspace() {
  return {
    proxyStatus: {
      running: false,
      lifecycle: "stopped",
      bindAddr: "127.0.0.1",
      port: 8787,
      startedAt: null,
      lastError: null,
      activeRequests: 0,
      requestCount: 0,
    },
    settings: {
      enabled: false,
      bindAddr: "127.0.0.1",
      port: 8787,
      endpoint: "chat_completions",
      policy: "automatic_balanced",
      maxRateMultiplier: null,
      routingGroupFilter: "all_groups",
      fallbackEnabled: true,
      previewKind: "baseline_eligibility",
    },
    summary: {
      candidateCount: 0,
      previewEligibleCandidateCount: 0,
      previewExcludedCandidateCount: 0,
      cooldownCandidateCount: 0,
      lastDecisionAt: null,
    },
    candidates: [],
    latestDecision: null,
    recentEvents: [],
  };
}
