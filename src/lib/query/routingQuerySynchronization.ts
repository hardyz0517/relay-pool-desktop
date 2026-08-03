import type { QueryClient } from "@tanstack/react-query";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import { routingQueryKeys } from "@/lib/queries/routingQueries";
import { queryKeys } from "./queryKeys";

type RoutingMutationResult = {
  localWorkspace?: LocalRoutingWorkspace;
};

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
    queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace }),
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
  result: RoutingMutationResult,
) {
  if (result.localWorkspace) {
    queryClient.setQueryData(queryKeys.localRoutingWorkspace, result.localWorkspace);
  }

  return refreshRoutingQueries(queryClient);
}
