import type { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import {
  ALERTING_READ_MODEL_UPDATED_EVENT,
  subscribeToAlertingReadModelUpdates,
} from "./AlertingReadModelSynchronizer";

function queryClientWithInvalidation(
  invalidateQueries: ReturnType<typeof vi.fn>,
): QueryClient {
  return { invalidateQueries } as unknown as QueryClient;
}

describe("subscribeToAlertingReadModelUpdates", () => {
  it("refreshes both cache families after subscribing and when a native update arrives", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);
    const queryClient = queryClientWithInvalidation(invalidateQueries);
    const unlisten = vi.fn();
    let listener: (() => void) | undefined;
    const subscribe = vi.fn(async (event: string, handler: () => void) => {
      expect(event).toBe(ALERTING_READ_MODEL_UPDATED_EVENT);
      listener = handler;
      return unlisten;
    });

    const returnedUnlisten = await subscribeToAlertingReadModelUpdates(queryClient, subscribe);

    expect(returnedUnlisten).toBe(unlisten);
    expect(invalidateQueries).toHaveBeenCalledTimes(2);

    listener?.();

    await vi.waitFor(() => {
      expect(invalidateQueries).toHaveBeenCalledTimes(4);
    });
    expect(invalidateQueries).toHaveBeenNthCalledWith(3, {
      queryKey: ["alertingCurrent"],
    });
    expect(invalidateQueries).toHaveBeenNthCalledWith(4, {
      queryKey: ["alertingActivity"],
    });
  });
});
