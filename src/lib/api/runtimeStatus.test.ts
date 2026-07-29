import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";
import type { RuntimeStatus } from "@/lib/types/runtimeStatus";

import { getRuntimeStatus } from "./runtimeStatus";

describe("runtime status backend cutover", () => {
  const status = runtimeStatus();
  const runtime = {
    getRuntimeStatus: vi.fn(async () => status),
  };

  beforeEach(() => {
    setActiveBackendClient({
      mode: "desktop",
      settings: {} as BackendClient["settings"],
      stations: {} as BackendClient["stations"],
      stationKeys: {} as BackendClient["stationKeys"],
      changeEvents: {} as BackendClient["changeEvents"],
      collectorRuns: {} as BackendClient["collectorRuns"],
      collectors: {} as BackendClient["collectors"],
      proxy: {} as BackendClient["proxy"],
      runtime,
      localRouting: {} as BackendClient["localRouting"],
      dataRecovery: {} as BackendClient["dataRecovery"],
      economics: {} as BackendClient["economics"],
      groupFacts: {} as BackendClient["groupFacts"],
      pricing: {} as BackendClient["pricing"],
      routing: {} as BackendClient["routing"],
      channels: {} as BackendClient["channels"],
      updater: {} as BackendClient["updater"],
      handshake: vi.fn(async () => ({}) as never),
    });
    runtime.getRuntimeStatus.mockClear();
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes runtime status reads through the active backend client", async () => {
    await expect(getRuntimeStatus()).resolves.toBe(status);

    expect(runtime.getRuntimeStatus).toHaveBeenCalledTimes(1);
  });
});

function runtimeStatus(): RuntimeStatus {
  return {
    tasks: [
      {
        id: "collector-loop",
        kind: "periodic",
        runId: 7,
        status: "running",
        lastStartedAtMs: 1234,
        lastSucceededAtMs: null,
        lastFailureCode: null,
        consecutiveFailures: 0,
        nextRetryAtMs: null,
      },
    ],
  };
}
