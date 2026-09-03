import { useCallback } from "react";
import { useMutation } from "@tanstack/react-query";
import { collectStationTask } from "@/lib/api/collector";
import { stationPublishedStatusQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";

const PUBLISHED_STATUS_TASK = "published_status" as const;

export function useStationPublishedStatus(stationId: string | null) {
  const workspaceQuery = useActivityQuery(
    stationPublishedStatusQueryOptions(stationId),
  );
  const { refetch: refetchWorkspace } = workspaceQuery;
  const refreshMutation = useMutation({
    mutationFn: async () => {
      if (!stationId) return;
      await collectStationTask(stationId, PUBLISHED_STATUS_TASK);
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
