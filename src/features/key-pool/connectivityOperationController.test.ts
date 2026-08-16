import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  cancelOperation: vi.fn(),
  getOperationStatus: vi.fn(),
  getStationKeyConnectivityOperationResult: vi.fn(),
  startStationKeyConnectivityOperation: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);

import {
  ConnectivityOperationCancelledError,
  runStationKeyConnectivityOperation,
} from "./connectivityOperationController";
import type { OperationSnapshotDto } from "@/lib/bridge/generated";
import type { StationKeyConnectivityTestResult } from "@/lib/types/stationKeys";

const result: StationKeyConnectivityTestResult = {
  stationKeyId: "key-1",
  ok: true,
  statusCode: 200,
  durationMs: 42,
  model: "gpt-4.1-mini",
  message: "ok",
  validatedProtocol: "responses",
  clientProfile: "standard_api",
  responseMode: "stream",
  streamFallbackReason: null,
};

function snapshot(progress: OperationSnapshotDto["progress"], terminal = false): OperationSnapshotDto {
  return {
    operationId: "1",
    kind: "station_key_connectivity",
    ownerFeature: "key-pool",
    state: terminal ? { state: "terminal", terminal: { terminal: "completed" } } : { state: "running" },
    progress,
    terminal: terminal ? { terminal: "completed" } : null,
  };
}

describe("connectivity operation controller", () => {
  beforeEach(() => {
    generated.cancelOperation.mockReset().mockResolvedValue({ outcome: "stopped", terminal: { terminal: "cancelled" } });
    generated.getOperationStatus.mockReset();
    generated.getStationKeyConnectivityOperationResult.mockReset().mockResolvedValue(result);
    generated.startStationKeyConnectivityOperation.mockReset().mockResolvedValue({ operationId: "1" });
  });

  it("reads the typed station-key result after the operation completes", async () => {
    const onEvent = vi.fn();
    generated.getOperationStatus.mockResolvedValueOnce(
      snapshot(
        [
          { sequence: 1, message: "attempt_started protocol=responses model=gpt-4.1-mini" },
        ],
        true,
      ),
    );

    await expect(
      runStationKeyConnectivityOperation(
        { stationKeyId: "key-1", model: "gpt-4.1-mini" },
        { onEvent, pollIntervalMs: 1 },
      ),
    ).resolves.toEqual(result);

    expect(generated.startStationKeyConnectivityOperation).toHaveBeenCalledWith({
      stationKeyId: "key-1",
      model: "gpt-4.1-mini",
    });
    expect(generated.getStationKeyConnectivityOperationResult).toHaveBeenCalledWith({ operationId: "1" });
    expect(onEvent).toHaveBeenCalledWith({
      type: "attemptStarted",
      protocol: "responses",
      model: "gpt-4.1-mini",
    });
    expect(onEvent).toHaveBeenCalledWith({ type: "completed", ok: true });
  });

  it("cancels the backend operation when the run is aborted", async () => {
    const abortController = new AbortController();
    generated.getOperationStatus.mockImplementationOnce(async () => {
      abortController.abort();
      return snapshot([]);
    });

    await expect(
      runStationKeyConnectivityOperation(
        { stationKeyId: "key-1", model: "gpt-4.1-mini" },
        { signal: abortController.signal, pollIntervalMs: 1 },
      ),
    ).rejects.toBeInstanceOf(ConnectivityOperationCancelledError);

    expect(generated.cancelOperation).toHaveBeenCalledWith({ operationId: "1", waitMs: 1000 });
  });

  it("does not replay a completed operation when its typed result is unavailable", async () => {
    generated.getOperationStatus.mockResolvedValueOnce(snapshot([], true));
    generated.getStationKeyConnectivityOperationResult.mockRejectedValueOnce(
      new Error("The operation outcome could not be confirmed."),
    );

    await expect(
      runStationKeyConnectivityOperation(
        { stationKeyId: "key-1", model: "gpt-4.1-mini" },
        { pollIntervalMs: 1 },
      ),
    ).rejects.toThrow("The operation outcome could not be confirmed.");

    expect(generated.startStationKeyConnectivityOperation).toHaveBeenCalledTimes(1);
    expect(generated.getStationKeyConnectivityOperationResult).toHaveBeenCalledTimes(1);
  });
});
