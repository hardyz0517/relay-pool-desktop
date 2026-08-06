import type { QueryClient } from "@tanstack/react-query";
import { routingQueryKeys } from "@/lib/queries/routingQueries";

export type RoutingQuerySynchronizationResult = {
  refreshed: boolean;
  errors: unknown[];
};

/**
 * Refresh every routing consumer through stable query-family prefixes.
 */
export async function refreshRoutingQueries(
  queryClient: QueryClient,
): Promise<RoutingQuerySynchronizationResult> {
  const refreshes = await Promise.allSettled([
    queryClient.invalidateQueries({ queryKey: routingQueryKeys.all }),
  ]);
  const errors = refreshes.flatMap((refresh) =>
    refresh.status === "rejected" ? [refresh.reason] : [],
  );

  return { refreshed: errors.length === 0, errors };
}

/**
 * Publish an authoritative mutation response immediately, then refresh all
 * routing read models so projections and runtime overlays converge with it.
 */
export function synchronizeRoutingQueriesAfterMutation(
  queryClient: QueryClient,
  _result?: unknown,
) {
  return refreshRoutingQueries(queryClient);
}
