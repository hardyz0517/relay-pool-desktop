import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  cancelOperation: vi.fn(),
  getOperationStatus: vi.fn(),
  getStationKeyModelDiscoveryOperationResult: vi.fn(),
  startStationKeyModelDiscoveryOperation: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);

import {
  ModelDiscoveryOperationCancelledError,
  runStationKeyModelDiscoveryOperation,
} from "./modelDiscoveryOperationController";

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
});
