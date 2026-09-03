import { useCallback, useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  cancelChannelMonitorExecution,
  runChannelMonitorNow,
} from "@/lib/api/channelMonitors";
import {
  channelStatusQueryOptions,
  currentStationBalanceSnapshotsQueryOptions,
} from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { invalidatePricingMonitoringQueries } from "@/lib/query/pricingMonitoringInvalidation";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type {
  ChannelStatusOutcome,
  ChannelStatusWorkspaceWindow,
} from "@/lib/types/channelMonitors";
import {
  buildChannelStatusWorkspaceView,
  createChannelStatusWorkspaceInput,
  defaultChannelStatusFilters,
  defaultChannelStatusSort,
  type ChannelStatusFilters,
  type ChannelStatusDisplayMode,
  type ChannelStatusRowView,
  type ChannelStatusSortModel,
} from "./channelStatusViewModel";
import {
  readChannelStatusWindow,
  writeChannelStatusWindow,
} from "./channelStatusWindowStorage";

export type ChannelStatusController = ReturnType<typeof useChannelStatusController>;
export type ChannelStatusTestScope = "enabled" | "with_balance";

export function useChannelStatusController(
  displayMode: Exclude<ChannelStatusDisplayMode, "both"> = "table",
) {
  const queryClient = useQueryClient();
  const [window, setWindowState] = useState<ChannelStatusWorkspaceWindow>(readChannelStatusWindow);
  const [filters, setFilters] = useState<ChannelStatusFilters>(defaultChannelStatusFilters);
  const [sort, setSort] = useState<ChannelStatusSortModel>(defaultChannelStatusSort);
  const [selectedExecutionId, setSelectedExecutionId] = useState<string | null>(null);
  const [batchTesting, setBatchTesting] = useState(false);
  const setWindow = useCallback((value: ChannelStatusWorkspaceWindow) => {
    setWindowState(value);
    writeChannelStatusWindow(value);
  }, []);

  const workspaceInput = useMemo(
    () => createChannelStatusWorkspaceInput({ window, filters, sort }),
    [filters, sort, window],
  );
  const statusQuery = useActivityQuery(channelStatusQueryOptions(5_000, workspaceInput));
  const workspaceView = useMemo(
    () => buildChannelStatusWorkspaceView(statusQuery.data, displayMode),
    [displayMode, statusQuery.data],
  );

  const runNowMutation = useMutation({
    mutationFn: async (row: ChannelStatusRowView) => {
      return runChannelMonitorNow(row.monitorId);
    },
    onSuccess: async () => {
      await invalidateMonitoringQueries(queryClient);
    },
  });

  const cancelMutation = useMutation({
    mutationFn: async (executionId: string) => cancelChannelMonitorExecution(executionId),
    onSuccess: async () => {
      await invalidateMonitoringQueries(queryClient);
    },
  });
  const runNowMutate = runNowMutation.mutate;
  const cancelMutate = cancelMutation.mutate;
  const refetchStatus = statusQuery.refetch;

  const runNow = useCallback((row: ChannelStatusRowView) => {
    if (row.runningExecutionId) {
      setSelectedExecutionId(row.runningExecutionId);
      return;
    }
    runNowMutate(row);
  }, [runNowMutate]);
  const cancel = useCallback((executionId: string) => {
    cancelMutate(executionId);
  }, [cancelMutate]);
  const setSearch = useCallback((value: string) => {
    setFilters((current) => ({ ...current, search: value }));
  }, []);
  const setEnabled = useCallback((value: ChannelStatusFilters["enabled"]) => {
    setFilters((current) => ({ ...current, enabled: value }));
  }, []);
  const setOutcome = useCallback((value: "all" | ChannelStatusOutcome) => {
    setFilters((current) => ({ ...current, outcome: value }));
  }, []);
  const refresh = useCallback(async () => {
    await refetchStatus({ throwOnError: true });
  }, [refetchStatus]);

  const testAll = useCallback(async (scope: ChannelStatusTestScope = "enabled") => {
    if (batchTesting) return;
    setBatchTesting(true);
    try {
      const snapshots = scope === "with_balance"
        ? await queryClient.fetchQuery(currentStationBalanceSnapshotsQueryOptions())
        : [];
      const rows = workspaceView.rows.filter((row) => scope === "enabled"
        ? row.enabled && !row.balancePaused
        : hasCurrentBalance(row, snapshots));
      if (rows.length === 0) return;
      await Promise.allSettled(rows.map((row) => runChannelMonitorNow(row.monitorId)));
      await invalidateMonitoringQueries(queryClient);
      await refetchStatus({ throwOnError: true });
    } finally {
      setBatchTesting(false);
    }
  }, [batchTesting, queryClient, refetchStatus, workspaceView.rows]);

  return {
    window,
    setWindow,
    filters,
    setSearch,
    setEnabled,
    setOutcome,
    sort,
    setSort,
    workspaceInput,
    statusQuery,
    workspaceView,
    selectedExecutionId,
    setSelectedExecutionId,
    isRunningAction: batchTesting || runNowMutation.isPending || cancelMutation.isPending,
    runNow,
    cancel,
    testAll,
    refresh,
  };
}

async function invalidateMonitoringQueries(queryClient: ReturnType<typeof useQueryClient>) {
  await Promise.all([
    invalidatePricingMonitoringQueries(queryClient),
    queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitorExecutions }),
  ]);
}

function hasCurrentBalance(
  row: ChannelStatusRowView,
  snapshots: Array<{ stationId: string; stationKeyId: string | null; value: number | null; totalValue: number | null; status: string }>,
) {
  const snapshot = snapshots.find((candidate) =>
    row.stationKeyId && candidate.stationKeyId
      ? candidate.stationKeyId === row.stationKeyId
      : candidate.stationId === row.stationId,
  );
  if (!snapshot) return false;
  return (snapshot.value ?? snapshot.totalValue ?? 0) > 0;
}
