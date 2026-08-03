import { describe, expect, it, vi } from "vitest";
import {
  refreshRoutingQueries,
  synchronizeRoutingQueriesAfterMutation,
} from "./routingQuerySynchronization";

describe("routing query synchronization", () => {
  it("publishes a returned workspace and refreshes every routing read model", async () => {
    const workspace = { candidates: [{ stationKeyId: "key-2" }, { stationKeyId: "key-1" }] };
    const setQueryData = vi.fn();
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);

    const result = await synchronizeRoutingQueriesAfterMutation(
      { setQueryData, invalidateQueries } as never,
      { localWorkspace: workspace as never },
    );

    expect(setQueryData).toHaveBeenCalledWith(["localRoutingWorkspace"], workspace);
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["localRoutingWorkspace"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["routing"] });
    expect(invalidateQueries).toHaveBeenCalledTimes(2);
    expect(result).toEqual({ refreshed: true, errors: [] });
  });

  it("refreshes both read-model families without a workspace payload", async () => {
    const setQueryData = vi.fn();
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);

    await refreshRoutingQueries({ setQueryData, invalidateQueries } as never);

    expect(setQueryData).not.toHaveBeenCalled();
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["localRoutingWorkspace"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["routing"] });
  });

  it("reports refresh failures without rejecting a persisted mutation", async () => {
    const refreshError = new Error("refresh failed");
    const invalidateQueries = vi
      .fn()
      .mockRejectedValueOnce(refreshError)
      .mockResolvedValueOnce(undefined);

    const result = await refreshRoutingQueries({
      setQueryData: vi.fn(),
      invalidateQueries,
    } as never);

    expect(result).toEqual({ refreshed: false, errors: [refreshError] });
    expect(invalidateQueries).toHaveBeenCalledTimes(2);
  });
});
