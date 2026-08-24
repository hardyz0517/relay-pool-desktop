import type { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { queryKeys } from "./queryKeys";
import { invalidateAlertingReadModels } from "./alertingQuerySynchronization";

function queryClientWithInvalidation(
  invalidateQueries: ReturnType<typeof vi.fn>,
): QueryClient {
  return { invalidateQueries } as unknown as QueryClient;
}

describe("invalidateAlertingReadModels", () => {
  it("invalidates both Change Center read-model families", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);

    const result = await invalidateAlertingReadModels(
      queryClientWithInvalidation(invalidateQueries),
    );

    expect(invalidateQueries).toHaveBeenNthCalledWith(1, {
      queryKey: queryKeys.alertingCurrentPrefix,
    });
    expect(invalidateQueries).toHaveBeenNthCalledWith(2, {
      queryKey: queryKeys.alertingActivityPrefix,
    });
    expect(result).toEqual({ refreshed: true, errors: [] });
  });

  it("still invalidates the other family when one refresh rejects", async () => {
    const error = new Error("current read model unavailable");
    const invalidateQueries = vi.fn()
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce(undefined);

    const result = await invalidateAlertingReadModels(
      queryClientWithInvalidation(invalidateQueries),
    );

    expect(invalidateQueries).toHaveBeenCalledTimes(2);
    expect(result).toEqual({ refreshed: false, errors: [error] });
  });
});
