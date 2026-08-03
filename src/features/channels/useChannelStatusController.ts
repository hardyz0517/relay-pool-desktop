import { useCallback, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  cancelChannelMonitorExecution,
  runChannelMonitorNowWithTrigger,
} from "@/lib/api/channelMonitors";
import {
  channelMonitorExecutionsQueryOptions,
  channelStatusQueryOptions,
} from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { invalidatePricingMonitoringQueries } from "@/lib/query/pricingMonitoringInvalidation";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type {
  ChannelMonitorExecutionListInput,
  ChannelStatusOutcome,
  ChannelStatusWorkspaceWindow,
} from "@/lib/types/channelMonitors";
import {
  buildChannelStatusWorkspaceView,
  createChannelStatusWorkspaceInput,
  defaultChannelStatusFilters,
  defaultChannelStatusSort,
  type ChannelStatusFilters,
  type ChannelStatusRowView,
  type ChannelStatusSortModel,
} from "./channelStatusViewModel";
import {
  readChannelStatusWindow,
  writeChannelStatusWindow,
} from "./channelStatusWindowStorage";

export type ChannelStatusController = ReturnType<typeof useChannelStatusController>;

export function useChannelStatusController() {
  const queryClient = useQueryClient();
  const [window, setWindowState] = useState<ChannelStatusWorkspaceWindow>(readChannelStatusWindow);
  const [filters, setFilters] = useState<ChannelStatusFilters>(defaultChannelStatusFilters);
  const [sort, setSort] = useState<ChannelStatusSortModel>(defaultChannelStatusSort);
  const [selectedExecutionId, setSelectedExecutionId] = useState<string | null>(null);
  const triggerRequestIds = useRef(new Map<string, string>());
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
    () => buildChannelStatusWorkspaceView(statusQuery.data),
    [statusQuery.data],
  );

  const executionListInput = useMemo<ChannelMonitorExecutionListInput>(
    () => (selectedExecutionId ? { limit: 50 } : { limit: 50 }),
    [selectedExecutionId],
  );
  const executionsQuery = useActivityQuery(channelMonitorExecutionsQueryOptions(executionListInput));

  const runNowMutation = useMutation({
    mutationFn: async (row: ChannelStatusRowView) => {
      const triggerRequestId = getOrCreateTriggerRequestId(triggerRequestIds.current, row.monitorId);
      return runChannelMonitorNowWithTrigger(row.monitorId, triggerRequestId);
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

  return {
    window,
    setWindow,
    filters,
    setSearch(value: string) {
      setFilters((current) => ({ ...current, search: value }));
    },
    setEnabled(value: ChannelStatusFilters["enabled"]) {
      setFilters((current) => ({ ...current, enabled: value }));
    },
    setOutcome(value: "all" | ChannelStatusOutcome) {
      setFilters((current) => ({ ...current, outcome: value }));
    },
    sort,
    setSort,
    workspaceInput,
    statusQuery,
    workspaceView,
    executions: executionsQuery.data?.items ?? [],
    executionsQuery,
    selectedExecutionId,
    setSelectedExecutionId,
    isRunningAction: runNowMutation.isPending || cancelMutation.isPending,
    runNow(row: ChannelStatusRowView) {
      if (row.runningExecutionId) {
        setSelectedExecutionId(row.runningExecutionId);
        return;
      }
      runNowMutation.mutate(row);
    },
    cancel(executionId: string) {
      cancelMutation.mutate(executionId);
    },
    async refresh() {
      await statusQuery.refetch({ throwOnError: true });
    },
  };
}

async function invalidateMonitoringQueries(queryClient: ReturnType<typeof useQueryClient>) {
  await Promise.all([
    invalidatePricingMonitoringQueries(queryClient),
    queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitorExecutions }),
  ]);
}

function getOrCreateTriggerRequestId(triggerRequestIds: Map<string, string>, monitorId: string) {
  const existing = triggerRequestIds.get(monitorId);
  if (existing) {
    return existing;
  }
  const requestId = `manual:${monitorId}:${Date.now()}:${Math.random().toString(36).slice(2, 10)}`;
  triggerRequestIds.set(monitorId, requestId);
  return requestId;
}
