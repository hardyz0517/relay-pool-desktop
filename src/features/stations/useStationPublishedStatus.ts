import { useCallback } from "react";
import { type QueryClient, useMutation, useQueryClient } from "@tanstack/react-query";
import { collectStationTask } from "@/lib/api/collector";
import { stationPublishedStatusQueryOptions } from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { useActivityQuery } from "@/lib/query/useActivityQuery";

const PUBLISHED_STATUS_TASK = "published_status" as const;

export async function invalidateStationPublishedStatusCollectionQueries(
  queryClient: QueryClient,
  stationId: string,
) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.stationPublishedStatus(stationId) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.collectorRuns(stationId) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.collectorSnapshots(stationId) }),
  ]);
}

export function useStationPublishedStatus(stationId: string | null) {
  const queryClient = useQueryClient();
  const workspaceQuery = useActivityQuery(
    stationPublishedStatusQueryOptions(stationId),
  );
  const { refetch: refetchWorkspace } = workspaceQuery;
  const refreshMutation = useMutation({
    mutationFn: async () => {
      if (!stationId) return;
      await collectStationTask(stationId, PUBLISHED_STATUS_TASK);
    },
    onSuccess: async () => {
      if (!stationId) return;
      await invalidateStationPublishedStatusCollectionQueries(queryClient, stationId);
    },
  });

  const refresh = useCallback(async () => {
    if (!stationId || refreshMutation.isPending) return;
    await refreshMutation.mutateAsync();
  }, [refreshMutation, stationId]);

  const retryWorkspace = useCallback(async () => {
    await refetchWorkspace();
  }, [refetchWorkspace]);

  return {
    workspace: workspaceQuery.data,
    isLoading: workspaceQuery.isPending,
    isError: workspaceQuery.isError,
    isRefreshing: refreshMutation.isPending,
    isRefreshError: refreshMutation.isError,
    refresh,
    retryWorkspace,
  };
}
