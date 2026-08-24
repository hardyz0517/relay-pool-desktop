import type { QueryClient } from "@tanstack/react-query";
import { queryKeys } from "@/lib/query/queryKeys";

export type AlertingQuerySynchronizationResult = {
  refreshed: boolean;
  errors: unknown[];
};

/**
 * Invalidate the two authoritative Change Center read-model families. This
 * refreshes every active filter variant, including the shell's unread badge.
 */
export async function invalidateAlertingReadModels(
  queryClient: QueryClient,
): Promise<AlertingQuerySynchronizationResult> {
  const refreshes = await Promise.allSettled([
    queryClient.invalidateQueries({ queryKey: queryKeys.alertingCurrentPrefix }),
    queryClient.invalidateQueries({ queryKey: queryKeys.alertingActivityPrefix }),
  ]);
  const errors = refreshes.flatMap((refresh) =>
    refresh.status === "rejected" ? [refresh.reason] : [],
  );

  return { refreshed: errors.length === 0, errors };
}
