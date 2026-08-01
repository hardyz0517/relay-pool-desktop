import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  cancelOperation: vi.fn(),
  getOperationStatus: vi.fn(),
  getStationKeyModelDiscoveryOperationResult: vi.fn(),
  startStationKeyModelDiscoveryOperation: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);

import {
  discoverAndPersistStationKeyModels,
  discoverCreatedStationKeyModels,
  ModelDiscoveryOperationCancelledError,
  runStationKeyModelDiscoveryOperation,
} from "@/lib/stationKeyModelDiscovery";

describe("model discovery operation controller", () => {
  beforeEach(() => {
    generated.cancelOperation.mockReset().mockResolvedValue({
      outcome: "stopped",
      terminal: { terminal: "cancelled" },
    });
    generated.startStationKeyModelDiscoveryOperation.mockReset().mockResolvedValue({ operationId: "7" });
    generated.getOperationStatus.mockReset().mockResolvedValue({
      operationId: "7",
      kind: "station_key_model_discovery",
      ownerFeature: "key-pool",
      state: { state: "terminal", terminal: { terminal: "completed" } },
      progress: [],
      terminal: { terminal: "completed" },
    });
    generated.getStationKeyModelDiscoveryOperationResult.mockReset().mockResolvedValue({
      stationKeyId: "key-1",
      models: ["gpt-5", "claude-sonnet"],
    });
  });

  it("returns the typed model list when discovery completes", async () => {
    await expect(
      runStationKeyModelDiscoveryOperation("key-1", { pollIntervalMs: 1 }),
    ).resolves.toEqual({
      stationKeyId: "key-1",
      models: ["gpt-5", "claude-sonnet"],
    });

    expect(generated.startStationKeyModelDiscoveryOperation).toHaveBeenCalledWith({
      stationKeyId: "key-1",
    });
    expect(generated.getStationKeyModelDiscoveryOperationResult).toHaveBeenCalledWith({
      operationId: "7",
    });
  });

  it("maps provider HTTP failures to an actionable message", async () => {
    generated.getOperationStatus.mockResolvedValueOnce({
      operationId: "7",
      kind: "station_key_model_discovery",
      ownerFeature: "key-pool",
      state: {
        state: "terminal",
        terminal: { terminal: "failed", code: "model-discovery-http" },
      },
      progress: [],
      terminal: { terminal: "failed", code: "model-discovery-http" },
    });

    await expect(
      runStationKeyModelDiscoveryOperation("key-1", { pollIntervalMs: 1 }),
    ).rejects.toThrow("请检查密钥权限和 API Base URL");
  });

  it("cancels the backend operation when discovery is aborted", async () => {
    const abortController = new AbortController();
    generated.getOperationStatus.mockImplementationOnce(async () => {
      abortController.abort();
      return {
        operationId: "7",
        kind: "station_key_model_discovery",
        ownerFeature: "key-pool",
        state: { state: "running" },
        progress: [],
        terminal: null,
      };
    });

    await expect(
      runStationKeyModelDiscoveryOperation("key-1", {
        pollIntervalMs: 1,
        signal: abortController.signal,
      }),
    ).rejects.toBeInstanceOf(ModelDiscoveryOperationCancelledError);

    expect(generated.cancelOperation).toHaveBeenCalledWith({ operationId: "7", waitMs: 1000 });
  });

  it("persists discovered models without choosing a default model", async () => {
    const updateCapabilities = vi.fn(async (input) => ({
      ...input,
      updatedAt: "2026-08-01T00:00:00Z",
    }));

    const result = await discoverAndPersistStationKeyModels("key-1", {
      runDiscovery: vi.fn(async () => ({
        stationKeyId: "key-1",
        models: [" gpt-5 ", "GPT-5", "claude-sonnet"],
      })),
      getCapabilities: vi.fn(async () => ({
        stationKeyId: "key-1",
        supportsChatCompletions: true,
        supportsResponses: true,
        supportsEmbeddings: false,
        supportsStream: true,
        supportsTools: true,
        supportsVision: false,
        supportsReasoning: true,
        modelAllowlist: [],
        modelBlocklist: ["blocked-model"],
        preferredModels: [],
        onlyUseAsBackup: false,
        routingTags: ["new"],
        updatedAt: "2026-08-01T00:00:00Z",
      })),
      updateCapabilities,
    });

    expect(result.models).toEqual(["claude-sonnet", "gpt-5"]);
    expect(updateCapabilities).toHaveBeenCalledWith(expect.objectContaining({
      stationKeyId: "key-1",
      modelAllowlist: ["claude-sonnet", "gpt-5"],
      modelBlocklist: ["blocked-model"],
      preferredModels: [],
      routingTags: ["new"],
    }));
  });

  it("keeps batch creation successful when one model discovery fails", async () => {
    const summary = await discoverCreatedStationKeyModels(
      ["key-1", "key-2", "key-1"],
      async (stationKeyId) => {
        if (stationKeyId === "key-2") {
          throw new Error("provider rejected models request");
        }
        return { stationKeyId, models: ["gpt-5"] };
      },
    );

    expect(summary).toMatchObject({
      requestedCount: 2,
      updatedCount: 1,
      emptyCount: 0,
      modelCount: 1,
    });
    expect(summary.failures).toHaveLength(1);
    expect(summary.failures[0].stationKeyId).toBe("key-2");
  });
});
